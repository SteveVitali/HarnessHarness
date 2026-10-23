//! R4.7a live leg — `oauth-device` (DF-S4.11-2; the 2026-10-20
//! "GitHub OAuth App" ruling; operator present to enter the code).
//! Gated per ADR-0363 — CI never sees a secret and the cells report
//! `unprovided — skipped` when `HH_GITHUB_OAUTH_CLIENT_ID` is absent.
//!
//! Resumable choreography (the operator leg cannot complete inside one
//! agent run): `initiate` runs the device-code request, writes the
//! `user_code`+`verification_uri`+deadline to `$HH_OAUTH_CODE_FILE`
//! (a gitignored operator-facing file — the code NEVER lands in a
//! transcript, record or ledger) and the `device_code` to
//! `$HH_OAUTH_STATE`; `complete` reloads the state and polls
//! `HH_OAUTH_POLL_CAP_S` seconds → the provider's access token →
//! `GET /user` subject → `OAUTH_SUBJECT_SHA256` + `OAUTH_RESULT`
//! markers (the token itself prints nowhere — exchange scope only).
//! `auto` is `initiate`+`complete` in one process (the operator is at
//! the terminal).
#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use hharness_mcp_lab::oauth::{
    self, BearerVerdict, DeviceFlow, OAuthIssuer, PollOutcome, Transport,
};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn client_id() -> Option<String> {
    let id = env("HH_GITHUB_OAUTH_CLIENT_ID")?;
    Some(id)
}

fn state_path() -> String {
    env("HH_OAUTH_STATE")
        .unwrap_or_else(|| format!("{}/hh-oauth-device.state", std::env::temp_dir().display()))
}

fn code_path() -> String {
    env("HH_OAUTH_CODE_FILE")
        .unwrap_or_else(|| format!("{}/hh-oauth-device.code", std::env::temp_dir().display()))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn result(r: &str) {
    println!("OAUTH_RESULT {r}");
}

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh oauth-device"]
fn r47a_oauth_device_initiate() {
    let Some(client_id) = client_id() else {
        println!("HH_GITHUB_OAUTH_CLIENT_ID unprovided — skipped");
        result("skipped(credential-unprovided)");
        return;
    };
    let flow = DeviceFlow::github();
    let grant = match oauth::initiate(Transport::Tls, &flow, &client_id) {
        Ok(g) => g,
        Err(oauth::OAuthError::Transport(e)) => {
            println!("initiate transport: {e}");
            result("skipped(unreachable: github.com)");
            return;
        }
        Err(e) => {
            println!("initiate: {e}");
            result("fail:initiate");
            return;
        }
    };
    // The poll credential persists for the resumable `complete` leg —
    // gitignored state, mode 0600; the operator-facing code goes to the
    // code file, never stdout/transcript/ledger.
    let deadline_ms = now_ms() + grant.expires_in_s * 1000;
    let state = format!(
        "{{\"device_code\":\"{}\",\"interval_s\":{},\"deadline_ms\":{}}}\n",
        grant.device_code, grant.interval_s, deadline_ms
    );
    std::fs::write(state_path(), &state).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(state_path(), std::fs::Permissions::from_mode(0o600));
    }
    std::fs::write(
        code_path(),
        format!(
            "GitHub device authorization pending —\n\
             go to {} and enter code: {}\n\
             (grant expires_in {}s — deadline epoch-ms {})\n",
            grant.verification_uri, grant.user_code, grant.expires_in_s, deadline_ms
        ),
    )
    .unwrap();
    println!("OAUTH_CODE_FILE {}", code_path());
    println!("OAUTH_INTERVAL_S {}", grant.interval_s);
    println!("OAUTH_DEADLINE_MS {deadline_ms}");
    result("initiated");
}

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh oauth-device"]
fn r47a_oauth_device_complete() {
    let Some(client_id) = client_id() else {
        println!("HH_GITHUB_OAUTH_CLIENT_ID unprovided — skipped");
        result("skipped(credential-unprovided)");
        return;
    };
    let Ok(state) = std::fs::read_to_string(state_path()) else {
        result("skipped(no-state — run initiate first)");
        return;
    };
    let j = hharness_wire::json::parse(&state).unwrap_or(hharness_wire::json::Json::Null);
    let device_code = j.get("device_code").and_then(|v| v.as_str()).unwrap_or("");
    let interval = j.get("interval_s").and_then(|v| v.as_int()).unwrap_or(5) as u64;
    let deadline_ms = j.get("deadline_ms").and_then(|v| v.as_int()).unwrap_or(0) as u64;
    if device_code.is_empty() || now_ms() >= deadline_ms {
        result("skipped(expired-state — re-run initiate)");
        return;
    }
    let cap_s: u64 = env("HH_OAUTH_POLL_CAP_S")
        .and_then(|v| v.parse().ok())
        .unwrap_or(840);
    let cap =
        Duration::from_secs(cap_s).min(Duration::from_millis(deadline_ms.saturating_sub(now_ms())));
    let flow = DeviceFlow::github();
    let out = match oauth::poll_until(
        Transport::Tls,
        &flow,
        &client_id,
        device_code,
        interval,
        Instant::now() + cap,
        &mut |d: Duration| std::thread::sleep(d),
    ) {
        Ok(o) => o,
        Err(oauth::OAuthError::Transport(e)) => {
            println!("poll transport: {e}");
            result("skipped(unreachable: github.com)");
            return;
        }
        Err(e) => {
            println!("poll: {e}");
            result("fail:poll");
            return;
        }
    };
    let PollOutcome::Granted(token) = out else {
        match out {
            PollOutcome::Denied(e) => result(&format!("fail:denied:{e}")),
            PollOutcome::Expired => {
                result("skipped(pending — operator did not authorize in window)")
            }
            PollOutcome::Granted(_) => unreachable!(),
        }
        return;
    };
    // The provider-verified subject — the token stays inside this
    // scope; only the sha256 of the login reports.
    match (OAuthIssuer {
        user_url: flow.user_url.clone(),
        transport: Transport::Tls,
    })
    .verify_bearer(&token)
    {
        BearerVerdict::Valid { subject } => {
            println!(
                "OAUTH_SUBJECT_SHA256 {}",
                hharness_wire::sha256::sha256_hex(subject.as_bytes())
            );
            println!("OAUTH_SUBJECT_LEN {}", subject.len());
            result("pass");
        }
        BearerVerdict::Invalid => result("fail:verify-refused"),
        BearerVerdict::Unreachable => result("skipped(unreachable: api.github.com)"),
    }
}
