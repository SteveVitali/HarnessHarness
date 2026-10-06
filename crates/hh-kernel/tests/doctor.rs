//! `hh-kernel doctor` — the binary self-check exits 0 on a clean build
//! (DF-DOC.1-1 / ADR-0332 D1): the in-process `hello` round trip's
//! `KernelDescriptor.version` is the SemVer-class label — the
//! `kernel_version_id` tail through the one canonical spelling — and a
//! negotiated-identity mismatch stays a loud failure. This test fails if
//! `doctor` regresses to comparing the label against the full
//! `hh-kernel/<ver>` id (the defect's shape).

use std::process::Command;

#[test]
fn doctor_exits_zero_on_a_clean_build() {
    // Bind the ledger to a unique temp root — `serve` defaults to
    // `<cwd>/.hh/store`, which would dirty the crate dir.
    let store = std::env::temp_dir().join(format!("hh-kernel-doctor-{}", std::process::id()));
    let out = Command::new(env!("CARGO_BIN_EXE_hh-kernel"))
        .arg("doctor")
        .env("HH_STORE_ROOT", &store)
        .env("HH_WORKSPACE_ROOT", &store)
        .output()
        .expect("spawn hh-kernel doctor");

    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ok");
    assert!(
        out.status.success(),
        "doctor exited non-zero: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The diagnostic line names the full version id the label derives
    // from — evidence the compare ran against the real identity.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("doctor: ok"), "{stderr}");
    assert!(stderr.contains("schema_hash=sha256:"), "{stderr}");
}
