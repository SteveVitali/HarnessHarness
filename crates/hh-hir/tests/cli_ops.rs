//! AC-IR-10 — every §3.1.7 operation is exercised **out-of-process over canonical bytes**
//! through the `hh-ir-op` binary: canonicalize, validate, identity, seal, diff, apply,
//! invert, migrate, project. In-crate results and process results must agree byte-for-byte.

mod common;

use common::*;

use hh_hir::document::parse_document;
use hh_hir::ops::canonicalize;
use hh_provenance::{PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_hh-ir-op")
}

fn tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("hh-ir-op-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

fn run(args: &[&std::path::Path]) -> (bool, Vec<u8>, String) {
    let strs: Vec<String> = args.iter().map(|p| p.display().to_string()).collect();
    let o = std::process::Command::new(bin())
        .args(&strs)
        .output()
        .expect("spawn hh-ir-op");
    // `out` prints the canonical bytes followed by a newline.
    let mut stdout = o.stdout;
    if stdout.last() == Some(&b'\n') {
        stdout.pop();
    }
    (
        o.status.success(),
        stdout,
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

fn path_arg(p: &std::path::Path) -> &std::path::Path {
    p
}

#[test]
fn canonicalize_validate_identity_over_the_process_boundary() {
    let doc = valid_doc();
    let doc_path = tmp("doc.json", &canonicalize(&doc));

    // canonicalize — must equal the in-crate result byte-for-byte.
    let (ok, stdout, err) = run(&[path_arg("canonicalize".as_ref()), &doc_path]);
    assert!(ok, "canonicalize failed: {err}");
    assert_eq!(stdout, canonicalize(&doc));

    // validate — a report with the node/edge counts.
    let (ok, stdout, err) = run(&[path_arg("validate".as_ref()), &doc_path]);
    assert!(ok, "validate failed: {err}");
    let report = String::from_utf8(stdout).unwrap();
    let want_nodes = format!("\"node_count\":{}", doc.nodes.len());
    let want_edges = format!("\"edge_count\":{}", doc.edges.len());
    assert!(report.contains(&want_nodes), "{report}");
    assert!(report.contains(&want_edges), "{report}");

    // identity — every node gains computed ids.
    let (ok, stdout, err) = run(&[path_arg("identity".as_ref()), &doc_path]);
    assert!(ok, "identity failed: {err}");
    let with_ids = parse_document(&stdout).unwrap();
    assert!(with_ids
        .nodes
        .iter()
        .all(|n| n.version.semantic_id.is_some() && n.version.version_id.is_some()));
}

#[test]
fn seal_diff_apply_invert_over_the_process_boundary() {
    let base = valid_doc();
    let mut target = base.clone();
    target.nodes.push(tool_node("test:tool2", 7));

    let base_path = tmp("base.json", &canonicalize(&base));
    let target_path = tmp("target.json", &canonicalize(&target));

    // seal — deterministic sealed definition bytes equal to the in-crate seal.
    let at = tmp("at.txt", b"1700000000");
    let _ = at;
    let sealed_in = hh_hir::seal(&base, 1700000000).unwrap();
    let sealed_in_bytes = sealed_in.canonical_bytes();
    // The CLI takes <doc> <sealed_at>: write the arg as a literal string path trick —
    // instead call it through a small shell-free wrapper: args are positional strings.
    let o = std::process::Command::new(bin())
        .args(["seal", &base_path.display().to_string(), "1700000000"])
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "seal failed: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    let mut seal_bytes = o.stdout.clone();
    seal_bytes.pop(); // trailing newline from `out`
    assert_eq!(seal_bytes, sealed_in_bytes);

    // diff — provenance record is a canonical ProvenanceRecord file.
    let prov = ProvenanceRecord::minted(
        hh_provenance::Origin::human("test:author", hh_provenance::HumanRole::Author),
        PersistenceScope::Run,
        9,
    );
    let prov_path = tmp("prov.json", prov.to_json().to_canonical_string().as_bytes());
    let o = std::process::Command::new(bin())
        .args([
            "diff",
            &base_path.display().to_string(),
            &target_path.display().to_string(),
            &prov_path.display().to_string(),
        ])
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "diff failed: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    let mut diff_bytes = o.stdout.clone();
    diff_bytes.pop();
    let diff_path = tmp("diff.json", &diff_bytes);

    // apply(base, diff) = target.
    let (ok, stdout, err) = run(&[path_arg("apply".as_ref()), &base_path, &diff_path]);
    assert!(ok, "apply failed: {err}");
    assert_eq!(stdout, canonicalize(&target));

    // invert + apply = base.
    let (ok, stdout, err) = run(&[path_arg("invert".as_ref()), &diff_path]);
    assert!(ok, "invert failed: {err}");
    let inv_path = tmp("inv.json", &stdout);
    let (ok, stdout, err) = run(&[path_arg("apply".as_ref()), &target_path, &inv_path]);
    assert!(ok, "apply(invert) failed: {err}");
    assert_eq!(stdout, canonicalize(&base));
}

#[test]
fn migrate_and_project_over_the_process_boundary() {
    let mut doc = valid_doc();
    doc.nodes.push(tool_node("test:tool", 9));
    let doc_path = tmp("mdoc.json", &canonicalize(&doc));

    // migrate is identity-only.
    let o = std::process::Command::new(bin())
        .args(["migrate", &doc_path.display().to_string(), "HIR/1", "HIR/1"])
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "migrate failed: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    let mut mig = o.stdout.clone();
    mig.pop();
    assert_eq!(mig, canonicalize(&doc));

    // project by kind keeps only matching nodes and their internal edges.
    let o = std::process::Command::new(bin())
        .args(["project", &doc_path.display().to_string(), "ToolCapability"])
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "project failed: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    let mut proj = o.stdout.clone();
    proj.pop();
    let j = hh_wire::canonical::parse_canonical(&proj).unwrap();
    let Json::Arr(nodes) = j.get("nodes").unwrap() else {
        panic!("nodes array")
    };
    assert_eq!(nodes.len(), 1);
    let Json::Arr(edges) = j.get("edges").unwrap() else {
        panic!("edges array")
    };
    assert!(edges.is_empty());

    // project by plane.
    let o = std::process::Command::new(bin())
        .args(["project", &doc_path.display().to_string(), "P1"])
        .output()
        .unwrap();
    assert!(o.status.success(), "project P1 failed");
}

#[test]
fn non_canonical_input_is_refused_over_the_boundary() {
    // A document re-serialized with non-canonical spacing/member order must fail.
    let doc = valid_doc();
    let mut noncanon = String::from_utf8(canonicalize(&doc)).unwrap();
    noncanon = noncanon.replacen("\"dialect\":", " \"dialect\" : ", 1);
    let p = tmp("noncanon.json", noncanon.as_bytes());
    let (ok, _stdout, err) = run(&[path_arg("validate".as_ref()), &p]);
    assert!(!ok, "non-canonical input must be refused, got success");
    assert!(err.contains("NonCanonicalInput"), "{err}");
}
