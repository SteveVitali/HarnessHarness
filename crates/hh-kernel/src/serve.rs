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
    let mut svc = EmbedService::open(ServiceConfig {
        store_root: store_root(),
        kernel_version_id: KERNEL_VERSION_ID.to_string(),
        workspace_root: workspace_root(),
        holder: KERNEL_VERSION_ID.to_string(),
    })
    .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    let mut buf = BufReader::new(reader);
    hh_embed::stdio::serve(&mut svc, &mut buf, &mut writer)
}
