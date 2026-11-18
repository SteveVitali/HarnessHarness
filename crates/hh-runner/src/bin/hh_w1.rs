//! `hh-w1` — the W1 window driver (manifest 290/R6.32; spec §10.1.4
//! row W1 (a); GATE-G25/PROV-W1). The on-VM acts the window performs:
//! the allowlist freeze (Q-X55), the carried binary-set and image
//! exports (REQ-XC-330), the observe pass's real-original leg
//! (REQ-XE-004/DF-R6.28-1), the §3.8 oracle re-proof over the real
//! `SurfaceDrive` (O = `harbor run -a oracle`, H-pub = `run_cell` on a
//! `declared_deps` podman cell, H-allow = `run_cell` on the
//! `allowlist_mediated` profile with the sealed phase schedule), the
//! REQ-XB-110 fault-injection leg, the REQ-XD-107 turn-1 census and
//! the REQ-XA-127 $0 targeted probe. Every record lands through the
//! committed codecs — no second implementation (CC1).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Instant;

use hharness_env::egress_allowlist::{
    admit_allowlist_mediated, cell_env_projection, scan_hosts_in_text, Allowlist, AllowlistFreeze,
    ImageFamily, MediatedCell, MediatedDecl, RelayPair, WithheldHost, EGRESS_RELAY_PORT,
    EGRESS_SOCK_PATH, MODEL_RELAY_PORT_MIN, MODEL_SOCK_PATH,
};
use hharness_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hharness_gateway::recording::{RecordedCall, RecordedReleased, RecordedResponse, Recording};
use hharness_helper::podman::{CellSpec, PodmanBackend, RelaySpec, SocketMount};
use hharness_runner::admission_build::{self, BuildPlan, InstallStep};
use hharness_runner::carried::{self, ExportPlan, ExportShape};
use hharness_runner::cell::{run_cell, CellError, CellExec, CellOps, CellOutcome, CellPlan};
use hharness_runner::harbor::{self, BlobSink, HarborPin, JobConfig};
use hharness_runner::json_ext::JsonExt;
use hharness_runner::reproof::{
    self, EvidenceLabel, ObservePass, ReproofPacket, ReproofPlan, SurfaceDrive, SurfaceOutcome,
};
use hharness_runner::verdict::{self, MediatorRow, RewardFileSet};
use hharness_wire::json::{self, Json};
use hharness_wire::sha256::sha256_hex;

// ── plumbing ────────────────────────────────────────────────────────────

fn usage() -> ! {
    eprintln!(
        "usage:\n  hh-w1 freeze --protocol <id> --epoch <n> --family <debian|ubuntu> --out <f> [--verify-extra a,b] [--withheld h=reason,...]\n  hh-w1 task-images --repo <ns> --tasks <csv> --out <f>\n  hh-w1 binary-set --bins <dir> --window <w> --out <dir>\n  hh-w1 image-export --role <bundle|layer_export|binary_set> --name <n> --image <name@sha256:..> --window <w> --out <dir> [--full]\n  hh-w1 image-build --kind <bundle|layer> --containerfile <f> --context <d> --tag <t> --out <dir>\n  hh-w1 observe --task <id> --mini-argv <tpl> --instruction <f> --responses <n> --out <dir>\n  hh-w1 o-run --task <id> --r <n> --cfg <f> --out <f>\n  hh-w1 h-run --task <id> --profile <pub|allow> --r <n> --cfg <f> --out <f>\n  hh-w1 prove --cfg <f> --tasks <csv> --r <n> --observe <mode> --out <dir> [--live-evidence-ref <ref>]\n  hh-w1 fault --cfg <f> --task <id> --out <f>\n  hh-w1 census --manifest <f> --work <dir> --out <f>\n  hh-w1 l1-fold --spec <f> --out <dir>\n  hh-w1 probe-run --cfg <f> --task <id> --out <f>\n  hh-w1 label-packet --packet <f> --ref <record-path>\n  hh-w1 index --artifacts <f> --out <f>\n  hh-w1 render-policy --freeze <f> --phase <setup|agent|verify> --out <f> [--model-port <n>] [--slot-token <t>] [--family <debian|ubuntu>]\n  hh-w1 scan-hosts [--files <a,b,…> | --list <f>]"
    );
    std::process::exit(2)
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn req(args: &[String], name: &str) -> String {
    arg(args, name).unwrap_or_else(|| {
        eprintln!("missing {name}");
        usage()
    })
}

fn read_json(path: &str) -> Result<Json, String> {
    let t = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    json::parse(&t).map_err(|e| format!("parse {path}: {e}"))
}

fn write_json(path: &Path, j: &Json) -> Result<(), String> {
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")));
    let bytes = j.to_canonical_string().into_bytes();
    std::fs::write(path, &bytes).map_err(|e| format!("write {}: {e}", path.display()))
}

fn sh(bin: &str, args: &[String]) -> Result<std::process::Output, String> {
    Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| format!("spawn {bin}: {e}"))
}

fn jstr<'a>(j: &'a Json, k: &str) -> Option<&'a str> {
    j.get(k).and_then(Json::as_str)
}

/// Directory blob sink for `ingest_job` — files land under `<dir>/`.
struct DirSink {
    dir: PathBuf,
}

impl BlobSink for DirSink {
    fn put(&mut self, bytes: &[u8], _mime: &str) -> Result<String, harbor::HarborError> {
        let sha = sha256_hex(bytes);
        let _ = std::fs::create_dir_all(&self.dir);
        let p = self.dir.join(&sha);
        std::fs::write(&p, bytes).map_err(|e| harbor::HarborError {
            detail: format!("blob write: {e}"),
        })?;
        Ok(format!("sha256:{sha}"))
    }
}

// ── cfg ─────────────────────────────────────────────────────────────────

struct Cfg {
    tasks_dir: String,
    harbor_bin: String,
    jobs_root: String,
    run_root: String,
    pin_file: String,
    images_file: String,
    #[allow(dead_code)]
    staging_root: String,
    mediator_bin: String,
    relay_bin: String,
    freeze_file: String,
    agent_deadline_ms: u64,
    verify_deadline_ms: u64,
    max_grade_attempts: u32,
    family: ImageFamily,
    model_port: u16,
    egress_port: u16,
    workdir_default: String,
    slot_token: String,
    node_dist_dir: String,
    python_dist_dir: String,
}

fn load_cfg(path: &str) -> Result<Cfg, String> {
    let j = read_json(path)?;
    let s = |k: &str| -> Result<String, String> {
        jstr(&j, k)
            .map(str::to_string)
            .ok_or_else(|| format!("cfg.{k} required"))
    };
    let n = |k: &str, d: u64| -> u64 {
        j.get(k)
            .and_then(Json::as_int)
            .map(|i| i as u64)
            .unwrap_or(d)
    };
    Ok(Cfg {
        tasks_dir: s("tasks_dir")?,
        harbor_bin: s("harbor_bin")?,
        jobs_root: s("jobs_root")?,
        run_root: s("run_root")?,
        pin_file: s("pin_file")?,
        images_file: s("images_file")?,
        staging_root: s("staging_root")?,
        mediator_bin: s("mediator_bin")?,
        relay_bin: s("relay_bin")?,
        freeze_file: s("freeze_file")?,
        agent_deadline_ms: n("agent_deadline_ms", 900_000),
        verify_deadline_ms: n("verify_deadline_ms", 600_000),
        max_grade_attempts: n("max_grade_attempts", 2) as u32,
        family: match jstr(&j, "family").unwrap_or("debian") {
            "ubuntu" => ImageFamily::Ubuntu,
            _ => ImageFamily::Debian,
        },
        model_port: n("model_port", MODEL_RELAY_PORT_MIN as u64) as u16,
        egress_port: n("egress_port", 3128) as u16,
        workdir_default: jstr(&j, "workdir_default").unwrap_or("/app").to_string(),
        slot_token: jstr(&j, "slot_token")
            .unwrap_or("hh-w1-slot-token")
            .to_string(),
        // Host-side node runtime tree copied into arms that declare
        // `runtime.node` (pi, claude-code) — the layer images carry no
        // runtime, so provisioning materializes the declared version.
        node_dist_dir: jstr(&j, "node_dist_dir")
            .unwrap_or("/opt/ws1/node")
            .to_string(),
        python_dist_dir: jstr(&j, "python_dist_dir")
            .unwrap_or("/opt/ws1/python")
            .to_string(),
    })
}

fn pin(cfg: &Cfg) -> Result<HarborPin, String> {
    let j = read_json(&cfg.pin_file)?;
    HarborPin::from_json(&j).map_err(|e| format!("pin: {e}"))
}

fn freeze(cfg: &Cfg) -> Result<AllowlistFreeze, String> {
    freeze_at(&cfg.freeze_file)
}

/// `freeze_at(path)` — the freeze-file parser `freeze` applies to
/// `cfg.freeze_file`, factored so `render-policy` reads the committed
/// artifact by the same code path (R6.45k).
fn freeze_at(path: &str) -> Result<AllowlistFreeze, String> {
    let j = read_json(path)?;
    let m = j
        .as_obj()
        .ok_or_else(|| "freeze must be an object".to_string())?;
    let list_j = m
        .get("list")
        .and_then(Json::as_obj)
        .ok_or_else(|| "freeze.list required".to_string())?;
    let hosts = |k: &str| -> Result<BTreeSet<String>, String> {
        Ok(list_j
            .get(k)
            .and_then(Json::as_arr)
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|h| h.as_str().map(str::to_string))
            .collect())
    };
    let list = Allowlist {
        agent: hosts("agent")?,
        verify: hosts("verify")?,
        permanent_denies: hosts("permanent_denies")?,
        withheld: list_j
            .get("withheld")
            .and_then(Json::as_arr)
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|w| {
                Some(WithheldHost {
                    host: w.get("host")?.as_str()?.to_string(),
                    reason: w.get("reason")?.as_str()?.to_string(),
                })
            })
            .collect(),
    };
    Ok(AllowlistFreeze {
        protocol_id: jstr(&j, "protocol_id")
            .ok_or_else(|| "freeze.protocol_id".to_string())?
            .to_string(),
        epoch: j
            .get("epoch")
            .and_then(Json::as_int)
            .map(|i| i as u64)
            .ok_or_else(|| "freeze.epoch".to_string())?,
        list,
        digest: jstr(&j, "digest")
            .ok_or_else(|| "freeze.digest".to_string())?
            .to_string(),
    })
}

/// `mediated_phase_schedule(pol, rec)` — the sealed `ext["phase_schedule"]`
/// after `admit_allowlist_mediated`: `h-run` and `render-policy` share
/// this extraction so the mediator's policy file is always the
/// transactional path's output, never a re-derivation (R6.45k).
fn mediated_phase_schedule(
    pol: &hharness_containment::policy::ContainmentPolicy,
    rec: &EnvironmentRecord,
) -> Result<Json, String> {
    pol.ext
        .get("phase_schedule")
        .or_else(|| rec.ext.get("phase_schedule"))
        .cloned()
        .ok_or_else(|| "phase_schedule absent after admit".to_string())
}

/// `phase_layer(sched, phase)` — one sealed layer document out of the
/// admitted `phase_schedule` (`setup`/`agent`/`verify`), parsed and
/// re-emitted canonically — the mediator's per-phase policy bytes.
fn phase_layer(sched: &Json, phase: &str) -> Result<Json, String> {
    let l = sched
        .get(phase)
        .and_then(|e| e.get("layer"))
        .cloned()
        .ok_or_else(|| format!("phase_schedule.{phase}"))?;
    let cp = hharness_containment::policy::ContainmentPolicy::from_json(&l)
        .map_err(|e| format!("layer {phase}: {e}"))?;
    Ok(cp.to_json())
}

fn task_image(cfg: &Cfg, task: &str) -> Result<String, String> {
    let j = read_json(&cfg.images_file)?;
    j.get(task)
        .and_then(|v| jstr(v, "image"))
        .map(str::to_string)
        .ok_or_else(|| format!("no image for task {task} in {}", cfg.images_file))
}

fn stage_task(cfg: &Cfg, task: &str, slot: &Path) -> Result<PathBuf, String> {
    let st = slot.join("staging");
    let _ = std::fs::create_dir_all(&st);
    let src = Path::new(&cfg.tasks_dir).join(task);
    for part in ["solution", "tests"] {
        let d = src.join(part);
        if d.is_dir() {
            let out = sh(
                "cp",
                &[
                    "-a".into(),
                    d.display().to_string(),
                    format!("{}/", st.display()),
                ],
            )?;
            if !out.status.success() {
                return Err(format!(
                    "stage {part}: {}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
        }
    }
    Ok(st)
}

// ── freeze ──────────────────────────────────────────────────────────────

fn cmd_freeze(args: &[String]) -> Result<String, String> {
    let protocol = req(args, "--protocol");
    let epoch: u64 = req(args, "--epoch").parse().map_err(|_| "epoch int")?;
    let family = match req(args, "--family").as_str() {
        "ubuntu" => ImageFamily::Ubuntu,
        _ => ImageFamily::Debian,
    };
    let verify_extra: BTreeSet<String> = arg(args, "--verify-extra")
        .map(|s| s.split(',').map(str::to_string).collect())
        .unwrap_or_default();
    let withheld: Vec<WithheldHost> = arg(args, "--withheld")
        .map(|s| {
            s.split(',')
                .filter_map(|p| {
                    p.split_once('=').map(|(h, r)| WithheldHost {
                        host: h.to_string(),
                        reason: r.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let list = Allowlist::first_pass(family, verify_extra, &[], withheld);
    let f = AllowlistFreeze::freeze(&protocol, epoch, list);
    write_json(Path::new(&req(args, "--out")), &f.to_json())?;
    Ok(Json::obj([("digest", Json::str(&f.digest))]).to_canonical_string())
}

/// `hh-w1 render-policy` — emit one sealed phase layer exactly as
/// `h-run` renders it for the mediator's `--policy` file (R6.45k's
/// conformance harness consumes these bytes): the committed freeze
/// parsed by `freeze_at`, `admit_allowlist_mediated` over the same
/// record + declaration shapes `h_run_once` builds, and the same
/// `mediated_phase_schedule`/`phase_layer` extraction — the policy
/// bytes are the transactional path's output, never re-derived.
fn cmd_render_policy(args: &[String]) -> Result<String, String> {
    let frz = freeze_at(&req(args, "--freeze"))?;
    let phase = req(args, "--phase");
    if !matches!(phase.as_str(), "setup" | "agent" | "verify") {
        return Err(format!(
            "render-policy: --phase must be setup|agent|verify, not {phase}"
        ));
    }
    let model_port: u16 = arg(args, "--model-port")
        .map(|s| s.parse::<u16>().map_err(|_| "--model-port int".to_string()))
        .transpose()?
        .unwrap_or(MODEL_RELAY_PORT_MIN);
    let slot_token = arg(args, "--slot-token").unwrap_or_else(|| "hh-w1-slot-token".to_string());
    let family = match arg(args, "--family").as_deref().unwrap_or("debian") {
        "ubuntu" => ImageFamily::Ubuntu,
        _ => ImageFamily::Debian,
    };
    // The record + declaration mirror h_run_once's — the record members
    // (image tag, cell socket paths, relay bin) stamp the record only;
    // the policy bytes ride the allowlist, model port and slot token.
    let mut rec = EnvironmentRecord {
        class: EnvironmentClass::LocalContainer,
        image: ImageRef::Tag("hh-w1-render-policy".to_string()),
        build_context: None,
        platform: "linux/amd64".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("w1".to_string(), "v0".to_string()),
        limits: hharness_containment::policy::ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
        canary_channels: vec![],
    };
    let mut pol = hharness_containment::policy::kernel_default(0);
    let decl = MediatedDecl {
        family,
        allowlist: frz.list.clone(),
        protocol_id: frz.protocol_id.clone(),
        epoch: frz.epoch,
        cell: MediatedCell {
            socket_mounts: BTreeMap::from([
                ("model".to_string(), MODEL_SOCK_PATH.to_string()),
                ("egress".to_string(), EGRESS_SOCK_PATH.to_string()),
            ]),
            relay_bin: "hh-cell-relay".to_string(),
            pairs: vec![
                RelayPair {
                    name: "model".to_string(),
                    port: model_port,
                },
                RelayPair {
                    name: "egress".to_string(),
                    port: EGRESS_RELAY_PORT,
                },
            ],
        },
        model_port,
        slot_token,
    };
    admit_allowlist_mediated(&mut rec, &mut pol, &decl, 0).map_err(|e| e.to_string())?;
    let sched = mediated_phase_schedule(&pol, &rec)?;
    let layer = phase_layer(&sched, &phase)?;
    let out = req(args, "--out");
    write_json(Path::new(&out), &layer)?;
    Ok(Json::obj([
        ("schema", Json::str("hh-w1-render-policy/1")),
        ("phase", Json::str(&phase)),
        ("out", Json::str(&out)),
        ("protocol_id", Json::str(&frz.protocol_id)),
        ("epoch", Json::Int(frz.epoch as i64)),
        ("freeze_digest", Json::str(&frz.digest)),
    ])
    .to_canonical_string())
}

/// `hh-w1 scan-hosts` — the mechanical `tests/*` host scan (R6.45k's
/// D16): the committed `scan_hosts_in_text` run over each named file,
/// unioned and emitted sorted one per line — the same function the
/// verify-set derivation cites, never a re-implementation.
fn cmd_scan_hosts(args: &[String]) -> Result<String, String> {
    let mut files: Vec<String> = arg(args, "--files")
        .map(|s| s.split(',').map(str::to_string).collect())
        .unwrap_or_default();
    if let Some(list) = arg(args, "--list") {
        let t = std::fs::read_to_string(&list).map_err(|e| format!("read {list}: {e}"))?;
        files.extend(
            t.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string),
        );
    }
    if files.is_empty() {
        return Err("scan-hosts: --files <a,b,…> or --list <f> required".to_string());
    }
    let mut hosts = BTreeSet::new();
    for f in &files {
        // tests/* carries binary fixtures (reference.jpg et al.) —
        // scan lossy: URL hosts are ASCII, replacement chars split
        // tokens exactly as the scan's own delimiters do.
        let bytes = std::fs::read(f).map_err(|e| format!("read {f}: {e}"))?;
        hosts.extend(scan_hosts_in_text(&String::from_utf8_lossy(&bytes)));
    }
    Ok(hosts.into_iter().collect::<Vec<_>>().join("\n"))
}

// ── task images ─────────────────────────────────────────────────────────

fn cmd_task_images(args: &[String]) -> Result<String, String> {
    let repo = req(args, "--repo");
    let tasks: Vec<String> = req(args, "--tasks")
        .split(',')
        .map(str::to_string)
        .collect();
    let mut m = BTreeMap::new();
    for t in &tasks {
        let ref_tag = format!("{repo}/{t}");
        let pull = sh("podman", &["pull".into(), ref_tag.clone()])?;
        if !pull.status.success() {
            m.insert(
                t.clone(),
                Json::obj([
                    ("ref", Json::str(&ref_tag)),
                    ("image", Json::Null),
                    (
                        "error",
                        Json::str(String::from_utf8_lossy(&pull.stderr).trim()),
                    ),
                ]),
            );
            continue;
        }
        let insp = sh(
            "podman",
            &["image".into(), "inspect".into(), ref_tag.clone()],
        )?;
        let ij = json::parse(&String::from_utf8_lossy(&insp.stdout))
            .map_err(|e| format!("inspect parse: {e}"))?;
        let digest = ij
            .as_arr()
            .and_then(|a| a.first())
            .and_then(|d| d.get("Digest"))
            .and_then(Json::as_str)
            .map(str::to_string)
            .ok_or_else(|| "inspect Digest".to_string())?;
        m.insert(
            t.clone(),
            Json::obj([
                ("ref", Json::str(&ref_tag)),
                ("image", Json::str(format!("{repo}/{t}@{digest}"))),
            ]),
        );
    }
    write_json(Path::new(&req(args, "--out")), &Json::Obj(m))?;
    Ok("task-images: written".to_string())
}

// ── exports ─────────────────────────────────────────────────────────────

fn cmd_binary_set(args: &[String]) -> Result<String, String> {
    let bins = PathBuf::from(req(args, "--bins"));
    let window = req(args, "--window");
    let out = PathBuf::from(req(args, "--out"));
    let _ = std::fs::create_dir_all(&out);
    let mut members = BTreeMap::new();
    for (name, _rel) in carried::INFRA_SET_MEMBERS {
        let b = std::fs::read(bins.join(name)).map_err(|e| format!("member {name}: {e}"))?;
        members.insert(name.to_string(), sha256_hex(&b));
    }
    let manifest = carried::set_manifest("infrastructure", &members)
        .map_err(|e| format!("set_manifest: {e}"))?;
    write_json(&out.join("binary-set.json"), &manifest)?;
    // the archive context: members + MANIFEST.sha256
    let ctx = out.join("ctx");
    let _ = std::fs::create_dir_all(&ctx);
    for (name, _) in carried::INFRA_SET_MEMBERS {
        let _ = std::fs::copy(bins.join(name), ctx.join(name));
    }
    let man_line: String = carried::INFRA_SET_MEMBERS
        .iter()
        .map(|(n, _)| format!("{}  {}", members[*n], n))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(ctx.join("MANIFEST.sha256"), format!("{man_line}\n"))
        .map_err(|e| e.to_string())?;
    let set_digest = members_into_digest(&manifest);
    let plan = ExportPlan {
        shape: ExportShape::ArchiveTar,
        role: "binary_set".to_string(),
        name: "infrastructure".to_string(),
        window: window.clone(),
        image: None,
        base_blobs: vec![],
        inputs: carried::INFRA_SET_MEMBERS
            .iter()
            .map(|(n, _)| n.to_string())
            .chain(std::iter::once("MANIFEST.sha256".to_string()))
            .collect(),
        context_dir: ctx.display().to_string(),
        out_path: out.join("infrastructure-set.tar").display().to_string(),
        identity: ("digest".to_string(), set_digest),
    };
    let rec = carried::run_export(&plan, &out.join("export.log"))
        .map_err(|e| format!("run_export: {e}"))?;
    write_json(&out.join("carried-export.json"), &rec)?;
    let bytes = std::fs::read(&plan.out_path).map_err(|e| e.to_string())?;
    let entry = plan
        .index_entry(&bytes, "archived", Some(&plan.out_path))
        .map_err(|e| format!("index_entry: {e}"))?;
    write_json(&out.join("index-entry.json"), &entry)?;
    Ok(Json::obj([("set_digest", Json::str(&plan.identity.1))]).to_canonical_string())
}

fn members_into_digest(manifest: &Json) -> String {
    manifest
        .get("set_digest")
        .and_then(Json::as_str)
        .unwrap_or("sha256:")
        .to_string()
}

fn cmd_image_export(args: &[String]) -> Result<String, String> {
    let role = req(args, "--role");
    let name = req(args, "--name");
    let image = req(args, "--image");
    let window = req(args, "--window");
    let out = PathBuf::from(req(args, "--out"));
    let _ = std::fs::create_dir_all(&out);
    // the config digest is the recorded identity — `podman image inspect`
    let insp = sh("podman", &["image".into(), "inspect".into(), image.clone()])?;
    let ij =
        json::parse(&String::from_utf8_lossy(&insp.stdout)).map_err(|e| format!("inspect: {e}"))?;
    let id = ij
        .as_arr()
        .and_then(|a| a.first())
        .and_then(|d| d.get("Id"))
        .and_then(Json::as_str)
        .map(str::to_string)
        .ok_or_else(|| "inspect Id".to_string())?;
    // podman's `.Id` is a bare 64-hex; the recorded identity is the
    // `sha256:<64>` form (REQ-XC-330 — a RepoDigest is never it).
    let id = format!(
        "sha256:{}",
        carried::image_id_normalize(&id).map_err(|e| format!("image_id: {e}"))?
    );
    let plan = ExportPlan {
        shape: if arg(args, "--full").is_some() {
            ExportShape::PodmanSave
        } else {
            ExportShape::BlobsPlusConfig
        },
        role: role.clone(),
        name: name.clone(),
        window,
        // `podman save` resolves `name@<RepoDigest>` for pulled images
        // and `sha256:<id>` for local builds — the resolved Id is the
        // uniform digest-pinned ref (R6.32b: a tag or a `name@<id>`
        // RepoDigest-shaped ref is not resolvable for local images).
        image: Some(id.clone()),
        base_blobs: vec![],
        inputs: vec![],
        context_dir: String::new(),
        out_path: out.join(format!("{name}.oci")).display().to_string(),
        identity: ("image_id".to_string(), id),
    };
    let rec = carried::run_export(&plan, &out.join("export.log"))
        .map_err(|e| format!("run_export: {e}"))?;
    write_json(&out.join("carried-export.json"), &rec)?;
    let bytes = if plan.shape == ExportShape::BlobsPlusConfig {
        dir_bytes(Path::new(&plan.out_path))
    } else {
        std::fs::read(&plan.out_path).map_err(|e| e.to_string())?
    };
    let entry = plan
        .index_entry(&bytes, "archived", Some(&plan.out_path))
        .map_err(|e| format!("index_entry: {e}"))?;
    write_json(&out.join("index-entry.json"), &entry)?;
    Ok(Json::obj([("image_id", Json::str(&plan.identity.1))]).to_canonical_string())
}

fn dir_bytes(root: &std::path::Path) -> Vec<u8> {
    let mut all = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if let Ok(b) = std::fs::read(&p) {
                    all.extend_from_slice(&b);
                }
            }
        }
    }
    all
}

fn cmd_image_build(args: &[String]) -> Result<String, String> {
    let kind = match req(args, "--kind").as_str() {
        "layer" => InstallStep::Layer,
        _ => InstallStep::Bundle,
    };
    let out = PathBuf::from(req(args, "--out"));
    let _ = std::fs::create_dir_all(&out);
    let plan = BuildPlan {
        kind,
        containerfile: req(args, "--containerfile"),
        context: req(args, "--context"),
        tag: req(args, "--tag"),
        iidfile: out.join("iidfile").display().to_string(),
    };
    let rec = admission_build::run_build(&plan, &out.join("build.log"))
        .map_err(|e| format!("run_build: {e}"))?;
    write_json(&out.join("harness-build.json"), &rec)?;
    let digest = jstr(&rec, "digest").unwrap_or("").to_string();
    // REQ-XC-304: the package-delta manifest rides the build record.
    if !digest.is_empty() {
        let argv = admission_build::package_delta_argv(&req(args, "--tag"));
        if let Ok(o) = sh("podman", &argv) {
            if let Ok(delta) = admission_build::parse_package_delta(
                &String::from_utf8_lossy(&o.stdout),
                &req(args, "--tag"),
            ) {
                let _ = write_json(&out.join("package-delta.json"), &delta);
            }
        }
    }
    Ok(Json::obj([("digest", Json::str(&digest))]).to_canonical_string())
}

// ── observe pass — the real-original leg (DF-R6.28-1) ───────────────────

fn openai_response(cmd: &str) -> Vec<u8> {
    Json::obj([
        (
            "choices",
            Json::Arr(vec![Json::obj([
                (
                    "message",
                    Json::obj([
                        ("role", Json::str("assistant")),
                        ("content", Json::str(format!("```bash\n{cmd}\n```"))),
                    ]),
                ),
                ("finish_reason", Json::str("stop")),
            ])]),
        ),
        ("model", Json::str("hh-recorded-response")),
    ])
    .to_canonical_string()
    .into_bytes()
}

fn serve_call(k: u64, body: Vec<u8>) -> RecordedCall {
    RecordedCall {
        k,
        downstream_method: "POST".to_string(),
        downstream_path: "/v1/chat/completions".to_string(),
        downstream_body: vec![],
        upstream_request: None,
        upstream_response: Some(RecordedResponse {
            status: 200,
            headers: BTreeMap::new(),
            body: body.clone(),
            chunk_stamps: vec![(0, body.len())],
        }),
        failure: None,
        released: Some(RecordedReleased {
            status: 200,
            headers: BTreeMap::new(),
            body,
        }),
        disposition: "forwarded".to_string(),
        latency_ms: 0,
    }
}

fn cmd_observe(args: &[String]) -> Result<String, String> {
    let task = req(args, "--task");
    let argv_tpl = req(args, "--mini-argv");
    let instruction = std::fs::read_to_string(req(args, "--instruction"))
        .map_err(|e| format!("instruction: {e}"))?;
    let n: u64 = req(args, "--responses").parse().unwrap_or(2);
    let out = PathBuf::from(req(args, "--out"));
    let _ = std::fs::create_dir_all(&out);
    // The scripted serving: turn commands then the submit marker — the
    // real agent's *requests* are the recorded evidence.
    let mut calls = Vec::new();
    for k in 0..n {
        let cmd = if k + 1 == n {
            "echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT"
        } else {
            "echo hh-w1-observe"
        };
        calls.push(serve_call(k, openai_response(cmd)));
    }
    let replay = hharness_runner::l1_drive::SequenceReplay::new(calls);
    let endpoint = hharness_runner::l1_drive::RecordedResponseEndpoint::bind(replay)
        .map_err(|e| format!("endpoint: {}", e.detail))?;
    let port = endpoint.port();
    let argv = argv_tpl
        .replace("{port}", &port.to_string())
        .replace("{instruction}", &instruction.replace('"', "'"));
    let t0 = Instant::now();
    let spawned = Command::new("sh")
        .args(["-c", &argv])
        .env("OPENAI_BASE_URL", format!("http://127.0.0.1:{port}/v1"))
        .env("OPENAI_API_BASE", format!("http://127.0.0.1:{port}/v1"))
        .env("OPENAI_API_KEY", "hh-w1-observe-token")
        .env("MSWEA_CONFIGURED", "true")
        .stdout(Stdio::from(
            std::fs::File::create(out.join("agent.stdout")).map_err(|e| e.to_string())?,
        ))
        .stderr(Stdio::from(
            std::fs::File::create(out.join("agent.stderr")).map_err(|e| e.to_string())?,
        ))
        .spawn();
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => return Err(format!("mini spawn: {e}")),
    };
    let deadline = Instant::now() + std::time::Duration::from_secs(600);
    let mut rc = None;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(s)) => {
                rc = Some(s.code().unwrap_or(-1));
                break;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(200)),
            Err(e) => return Err(format!("wait: {e}")),
        }
    }
    if rc.is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let captured = endpoint.captured();
    drop(endpoint);
    // the committed recording — every captured turn's upstream-bound
    // body is the DF-R6.28-1 evidence.
    let rec_calls: Vec<RecordedCall> = captured
        .iter()
        .enumerate()
        .map(|(k, c)| RecordedCall {
            k: k as u64,
            downstream_method: c.method.clone(),
            downstream_path: c.path.clone(),
            downstream_body: c.body.clone(),
            upstream_request: Some(c.upstream_body.clone()),
            upstream_response: None,
            failure: None,
            released: None,
            disposition: "forwarded".to_string(),
            latency_ms: 0,
        })
        .collect();
    let rec = Recording {
        slot_id: format!("w1-observe-{task}"),
        provenance: "recorded".to_string(),
        calls: rec_calls,
    };
    let rec_j = rec.to_json();
    write_json(&out.join(format!("{task}.recording.json")), &rec_j)?;
    Ok(Json::obj([
        ("task", Json::str(&task)),
        ("turns", Json::Int(captured.len() as i64)),
        (
            "upstream_body_bytes_per_turn",
            Json::Arr(
                captured
                    .iter()
                    .map(|c| Json::Int(c.upstream_body.len() as i64))
                    .collect::<Vec<_>>(),
            ),
        ),
        ("agent_exit", Json::Int(rc.unwrap_or(-99) as i64)),
        ("wall_ms", Json::Int(t0.elapsed().as_millis() as i64)),
        (
            "recording_sha256",
            Json::str(format!(
                "sha256:{}",
                sha256_hex(&rec_j.to_canonical_string().into_bytes())
            )),
        ),
    ])
    .to_canonical_string())
}

// ── surface trials ──────────────────────────────────────────────────────

fn outcome_json(o: &SurfaceOutcome) -> Json {
    Json::obj([
        (
            "rewards",
            Json::Arr(o.rewards.iter().map(|b| Json::Bool(*b)).collect()),
        ),
        (
            "digests",
            Json::Obj(
                o.digests
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                    .collect(),
            ),
        ),
        (
            "denied_hosts",
            Json::Arr(
                o.denied_hosts
                    .iter()
                    .map(|h| Json::str(h.clone()))
                    .collect(),
            ),
        ),
        (
            "harbor_exception",
            o.harbor_exception
                .as_ref()
                .map(|e| Json::str(e.clone()))
                .unwrap_or(Json::Null),
        ),
        ("wall_ms", Json::Int(o.wall_ms as i64)),
    ])
}

fn o_run(cfg: &Cfg, task: &str, r: u32, work: &Path) -> Result<SurfaceOutcome, String> {
    let t0 = Instant::now();
    let p = pin(cfg)?;
    let job_name = format!("w1-o-{}-{}", task.replace('_', "-"), r);
    let cfg_j = JobConfig::new(
        &p,
        &job_name,
        &cfg.tasks_dir,
        vec![task.to_string()],
        "oracle",
        r,
        r,
        &cfg.jobs_root,
        None,
    )
    .map_err(|e| format!("jobconfig: {e}"))?;
    let argv = cfg_j.to_argv(&cfg.harbor_bin).map_err(|e| e.to_string())?;
    let (bin, rest) = argv.split_first().ok_or_else(|| "argv".to_string())?;
    let out = Command::new(bin)
        .args(rest)
        .stdout(Stdio::from(
            std::fs::File::create(work.join("harbor.stdout")).map_err(|e| e.to_string())?,
        ))
        .stderr(Stdio::from(
            std::fs::File::create(work.join("harbor.stderr")).map_err(|e| e.to_string())?,
        ))
        .status()
        .map_err(|e| format!("harbor spawn: {e}"))?;
    let mut sink = DirSink {
        dir: work.join("blobs"),
    };
    let ingest = harbor::ingest_job(Path::new(&cfg.jobs_root), &job_name, &cfg_j, &p, &mut sink)
        .map_err(|e| format!("ingest: {e}"))?;
    write_json(&work.join("harbor-ingest.json"), &ingest.to_json())?;
    let mut o = SurfaceOutcome {
        rewards: Vec::new(),
        digests: BTreeMap::new(),
        denied_hosts: vec![],
        harbor_exception: None,
        wall_ms: t0.elapsed().as_millis() as u64,
    };
    let image = task_image(cfg, task).unwrap_or_default();
    o.digests.insert("image".to_string(), image);
    if !out.success() && ingest.trials.is_empty() {
        o.harbor_exception = Some(format!("harbor rc={}", out.code().unwrap_or(-1)));
        o.rewards = (0..r).map(|_| false).collect();
        return Ok(o);
    }
    for t in &ingest.trials {
        o.rewards.push(t.reward_ppm.map(|p| p > 0).unwrap_or(false));
        if let Some(e) = &t.exception {
            if o.harbor_exception.is_none() {
                o.harbor_exception = t.cause.clone().or(Some(e.sha256.clone()));
            }
        }
        if let Some(c) = &t.cause {
            if t.cause_mapped && o.harbor_exception.is_none() {
                o.harbor_exception = Some(c.clone());
            }
        }
        for (kind, d) in [
            (
                "reward",
                t.reward_files
                    .get("reward.json")
                    .or(t.reward_files.get("reward.txt")),
            ),
            ("ctrf", t.ctrf.as_ref()),
            ("stdout", t.test_stdout.as_ref()),
        ] {
            if let Some(d) = d {
                o.digests.insert(kind.to_string(), d.sha256.clone());
            }
        }
    }
    while (o.rewards.len() as u32) < r {
        o.rewards.push(false);
        if o.harbor_exception.is_none() {
            o.harbor_exception = Some(format!("trials_shortfall:{}<{}", ingest.trials.len(), r));
        }
    }
    Ok(o)
}

/// Env-injecting cell — PodmanBackend execs under the declared env
/// projection (the mediated profile's routing hints). The phase file
/// swap rides `exec` argv identity: the verifier argv writes the
/// verify layer, the agent argv the agent layer (the mediator re-reads
/// per connection — REQ-XB-113).
struct DriveCell<'a> {
    backend: &'a PodmanBackend,
    env: Vec<(String, String)>,
    /// A distinct env for the verifier phase (hosted-arm cells: the
    /// agent's closed render carries the slot token — it never enters
    /// the verify phase's environment).
    verify_env: Option<Vec<(String, String)>>,
    _agent_argv: Vec<String>,
    verifier_argv: Vec<String>,
    policy_dst: Option<PathBuf>,
    agent_policy: Option<Json>,
    verify_policy: Option<Json>,
}

impl CellOps for DriveCell<'_> {
    fn present(&mut self, p: &str) -> Result<bool, CellError> {
        self.backend.exists(p).map_err(|e| CellError {
            op: "exists",
            detail: e.to_string(),
        })
    }
    fn ls_dir(&mut self, p: &str) -> Result<Vec<String>, CellError> {
        self.backend.ls_dir(p).map_err(|e| CellError {
            op: "ls",
            detail: e.to_string(),
        })
    }
    fn cp_in(&mut self, s: &Path, d: &str) -> Result<(), CellError> {
        self.backend
            .cp_in(&s.to_string_lossy(), d)
            .map_err(|e| CellError {
                op: "cp_in",
                detail: e.to_string(),
            })
    }
    fn cp_out(&mut self, s: &str, d: &Path) -> Result<(), CellError> {
        self.backend
            .cp_out(s, &d.to_string_lossy())
            .map_err(|e| CellError {
                op: "cp_out",
                detail: e.to_string(),
            })
    }
    fn rm_glob(&mut self, g: &str) -> Result<(), CellError> {
        self.backend.rm_glob(g).map_err(|e| CellError {
            op: "rm_glob",
            detail: e.to_string(),
        })
    }
    fn exec(&mut self, spec: &CellExec) -> Result<CellOutcome, CellError> {
        // sealed phase schedule: the verify exec runs under the verify
        // layer — the policy file is the mediator's per-connection read.
        if let (Some(dst), Some(vp), Some(ap)) =
            (&self.policy_dst, &self.verify_policy, &self.agent_policy)
        {
            let layer = if spec.argv == self.verifier_argv {
                vp
            } else {
                ap
            };
            let _ = std::fs::write(dst, layer.to_canonical_string());
        }
        // The D17 lapse machinery is the shared `exec_guarded`: the
        // exec's own in-cell tree dies by its `HH_EXEC_TAG` environ
        // marker (never a cmdline pattern the phase may not carry),
        // death is /proc-confirmed, and the lapsed streams are kept.
        let env = match (&self.verify_env, spec.argv == self.verifier_argv) {
            (Some(ve), true) => ve,
            _ => &self.env,
        };
        hharness_runner::cell::exec_guarded(
            self.backend,
            &spec.argv,
            &spec.cwd,
            env,
            spec.deadline_ms,
        )
    }
    fn terminate_group(&mut self, pat: &str) -> Result<(), CellError> {
        self.backend.terminate_group(pat).map_err(|e| CellError {
            op: "terminate",
            detail: e.to_string(),
        })
    }
}

struct MediatorGuard {
    child: Child,
}

impl Drop for MediatorGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `collect_denied` — read the mediator run's durable rows and fold
/// the verify-phase denies into the denied-host set (the row fold is
/// [`verdict::denied_hosts`]; `verify_version_id` is the verify
/// layer's policy `version_id` — only denies decided under it are
/// verifier-bootstrap evidence).
fn collect_denied(store_root: &Path, run_id: &str, verify_version_id: Option<&str>) -> Vec<String> {
    if let Ok(store) = hharness_ledger::store::Store::open(store_root) {
        if store.has_run(run_id) {
            if let Ok(events) = store.events(run_id) {
                return verdict::denied_hosts(
                    events.iter().map(|e| (e.class.as_str(), &e.payload)),
                    verify_version_id,
                );
            }
        }
    }
    vec![]
}

fn h_run_once(
    cfg: &Cfg,
    task: &str,
    mediated: bool,
    slot: &Path,
    override_verify_drop: Option<&str>,
) -> Result<(bool, RewardFileSet, Vec<MediatorRow>, Vec<String>), String> {
    let staging = stage_task(cfg, task, slot)?;
    let collect = slot.join("collect");
    let work = slot.join("work");
    let _ = std::fs::create_dir_all(&work);
    let image = task_image(cfg, task)?;
    let container = format!(
        "hh-w1-{}-{}",
        task.replace('_', "-").chars().take(24).collect::<String>(),
        if mediated { "allow" } else { "pub" }
    );
    let agent_argv = vec!["bash".to_string(), "/solution/solve.sh".to_string()];
    let verifier_argv = vec!["bash".to_string(), "/tests/test.sh".to_string()];
    let mut env: Vec<(String, String)> = Vec::new();
    let mut mounts = Vec::new();
    let mut relay = None;
    let mut policy_dst = None;
    let mut agent_policy = None;
    let mut verify_policy = None;
    let mut verify_version_id: Option<String> = None;
    let mut guard: Option<MediatorGuard> = None;
    let mut store_root = PathBuf::new();
    let run_id = format!("w1-{}", container);
    if mediated {
        // admit_allowlist_mediated over a local_container record — the
        // declared transactional path (REQ-XB-113).
        let mut rec = EnvironmentRecord {
            class: EnvironmentClass::LocalContainer,
            image: ImageRef::Tag(image.clone()),
            build_context: None,
            platform: "linux/amd64".to_string(),
            provisioning: ProvisioningRecipe::default(),
            containment_policy_ref: ("w1".to_string(), "v0".to_string()),
            limits: hharness_containment::policy::ResourceLimits::default(),
            nondeterminism: vec![],
            unpinned: BTreeSet::new(),
            image_attestation: None,
            ext: BTreeMap::new(),
            canary_channels: vec![],
        };
        let mut pol = hharness_containment::policy::kernel_default(0);
        let frz = freeze(cfg)?;
        let mut list = frz.list.clone();
        if let Some(drop_host) = override_verify_drop {
            list.verify.remove(drop_host);
        }
        let model_sock = slot.join("model.sock");
        let egress_sock = slot.join("egress.sock");
        let cell = MediatedCell {
            socket_mounts: BTreeMap::from([
                ("model".to_string(), model_sock.display().to_string()),
                ("egress".to_string(), egress_sock.display().to_string()),
            ]),
            relay_bin: cfg.relay_bin.clone(),
            pairs: vec![
                RelayPair {
                    name: "model".to_string(),
                    port: cfg.model_port,
                },
                RelayPair {
                    name: "egress".to_string(),
                    port: cfg.egress_port,
                },
            ],
        };
        let decl = MediatedDecl {
            family: cfg.family,
            allowlist: list,
            protocol_id: frz.protocol_id.clone(),
            epoch: frz.epoch,
            cell,
            model_port: cfg.model_port,
            slot_token: cfg.slot_token.clone(),
        };
        admit_allowlist_mediated(&mut rec, &mut pol, &decl, 0).map_err(|e| e.to_string())?;
        // the sealed phase layers → the mediator's policy file (the
        // shared render — `hh-w1 render-policy` emits the same bytes
        // through `mediated_phase_schedule`, never a re-derivation;
        // R6.45k).
        let sched = mediated_phase_schedule(&pol, &rec)?;
        agent_policy = Some(phase_layer(&sched, "agent")?);
        verify_policy = Some(phase_layer(&sched, "verify")?);
        // The verify layer's `version_id` — the `policy_version_id`
        // the mediator stamps on each row it decides under that layer.
        verify_version_id = verify_policy
            .as_ref()
            .and_then(|p| p.get("version_id"))
            .and_then(Json::as_str)
            .map(str::to_string);
        let policy_file = slot.join("policy.json");
        std::fs::write(
            &policy_file,
            agent_policy.as_ref().unwrap().to_canonical_string(),
        )
        .map_err(|e| e.to_string())?;
        policy_dst = Some(policy_file.clone());
        // model stub — the mount's host socket must exist; the oracle
        // participant never dials it.
        let _stub = Command::new("python3")
            .args([
                "-c",
                &format!(
                    "import socket,time\ns=socket.socket(socket.AF_UNIX)\ns.bind('{}')\ns.listen(1)\ntime.sleep(10**6)",
                    model_sock.display()
                ),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        store_root = slot.join("mediator-store");
        let med = Command::new(&cfg.mediator_bin)
            .args([
                "--store-root".to_string(),
                store_root.display().to_string(),
                "--socket".to_string(),
                egress_sock.display().to_string(),
                "--policy".to_string(),
                policy_file.display().to_string(),
                "--env-handle".to_string(),
                format!("env-{run_id}"),
                "--effect-id".to_string(),
                format!("eff-{run_id}"),
                "--participant".to_string(),
                "oracle".to_string(),
                "--run-id".to_string(),
                run_id.clone(),
            ])
            .stdout(Stdio::from(
                std::fs::File::create(slot.join("mediator.log")).map_err(|e| e.to_string())?,
            ))
            .stderr(Stdio::from(
                std::fs::File::create(slot.join("mediator.err")).map_err(|e| e.to_string())?,
            ))
            .spawn()
            .map_err(|e| format!("mediator spawn: {e}"))?;
        guard = Some(MediatorGuard { child: med });
        // The socket file must exist before `podman create`: a `-v`
        // mount whose source is absent is silently created as a
        // *directory*, and the relay's upstream connect then fails on
        // every request (the Wc 0/R signature). Wait for the real
        // socket — a mediator that never binds fails the slot as a
        // machinery error, never as a phantom zero-request run.
        let sock_deadline = Instant::now() + std::time::Duration::from_secs(10);
        while !egress_sock.exists() {
            if Instant::now() > sock_deadline {
                return Err(format!(
                    "mediator egress socket {} never bound",
                    egress_sock.display()
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        mounts = vec![
            SocketMount {
                name: "model".to_string(),
                host_path: model_sock.display().to_string(),
            },
            SocketMount {
                name: "egress".to_string(),
                host_path: egress_sock.display().to_string(),
            },
        ];
        relay = Some(RelaySpec {
            host_bin: cfg.relay_bin.clone(),
            pairs: vec![
                ("model".to_string(), cfg.model_port),
                ("egress".to_string(), cfg.egress_port),
            ],
        });
        env = cell_env_projection(cfg.model_port, &cfg.slot_token)
            .map_err(|e| e.to_string())?
            .into_iter()
            .collect::<Vec<(String, String)>>();
        // The projection binds the declared `EGRESS_RELAY_PORT` (3128);
        // the relay binds `cfg.egress_port` — when the drive config
        // names a different port the proxy env must point at the port
        // the relay actually bound, or every egress connect is refused
        // before it ever reaches the mediator (the Wc dev-leg 3130/8090
        // cells). Same pair the projection mints, one authoritative port.
        for (k, v) in &mut env {
            if matches!(
                k.as_str(),
                "http_proxy" | "https_proxy" | "HTTP_PROXY" | "HTTPS_PROXY"
            ) {
                *v = format!("http://127.0.0.1:{}", cfg.egress_port);
            }
        }
    }
    // D15/D18 — the task's own `task.toml` declarations drive the
    // enforced deadlines (pinned Harbor semantics: multiplier 1.0,
    // the cfg value is the absent-value fallback) and the cell's
    // declared cpu/memory limits.
    let decl = hharness_runner::task_toml::load_task_decl(
        &std::path::Path::new(&cfg.tasks_dir).join(task),
    )?;
    let (agent_ms, agent_src) = hharness_runner::task_toml::resolve_deadline_ms(
        decl.agent_timeout_sec,
        cfg.agent_deadline_ms,
    );
    let (verify_ms, verify_src) = hharness_runner::task_toml::resolve_deadline_ms(
        decl.verifier_timeout_sec,
        cfg.verify_deadline_ms,
    );
    let backend = PodmanBackend {
        container: container.clone(),
        image,
        workspace: work.display().to_string(),
        network: !mediated,
        extra_args: vec![],
        cell: CellSpec {
            workdir: None,
            cpus: decl.cpus.clone(),
            memory_mb: decl.memory_mb,
        },
        mounts,
        relay,
    };
    backend.create().map_err(|e| e.to_string())?;
    backend.start().map_err(|e| e.to_string())?;
    let workdir = backend
        .image_workdir()
        .unwrap_or_else(|_| cfg.workdir_default.clone());
    let mut dc = DriveCell {
        backend: &backend,
        env,
        verify_env: None,
        _agent_argv: agent_argv.clone(),
        verifier_argv: verifier_argv.clone(),
        policy_dst,
        agent_policy,
        verify_policy,
    };
    let plan = CellPlan {
        oracle_participant: true,
        workdir,
        host_staging: staging,
        collect_dir: collect.clone(),
        agent_argv,
        agent_deadline_ms: Some(agent_ms),
        agent_deadline_source: agent_src,
        verifier_argv,
        verifier_deadline_ms: Some(verify_ms),
        verifier_deadline_source: verify_src,
        max_grade_attempts: cfg.max_grade_attempts,
        logs_copyout: None,
    };
    let report = run_cell(&mut dc, &plan).map_err(|e| format!("run_cell: {e}"))?;
    let _ = backend.stop();
    drop(guard);
    let files = report
        .verify
        .as_ref()
        .map(|v| v.files.clone())
        .unwrap_or_default();
    let timed_out = report.verify.as_ref().map(|v| v.timed_out).unwrap_or(false);
    let denied = if mediated {
        collect_denied(&store_root, &run_id, verify_version_id.as_deref())
    } else {
        vec![]
    };
    let rows: Vec<MediatorRow> = denied
        .iter()
        .map(|_| MediatorRow {
            phase: "verify".to_string(),
            decision: "deny".to_string(),
        })
        .collect();
    let v = verdict::classify(&files, &rows, timed_out);
    let passed = v.success.unwrap_or(false);
    // record the cell drive report
    let deadline_member = |ms: Option<u64>, src: hharness_runner::cell::DeadlineSource| {
        Json::obj([
            (
                "deadline_ms",
                ms.map(|m| Json::Int(m as i64)).unwrap_or(Json::Null),
            ),
            ("deadline_source", Json::str(src.as_str())),
        ])
    };
    let rep = Json::obj([
        (
            "agent_end_cause",
            Json::str(report.agent_end_cause.as_str()),
        ),
        ("verify_attempts", Json::Int(report.verify_attempts as i64)),
        (
            "verify_attempt",
            report
                .verify
                .as_ref()
                .map(|o| Json::Int(o.attempt as i64))
                .unwrap_or(Json::Null),
        ),
        (
            "deadlines",
            Json::obj([
                (
                    "agent",
                    deadline_member(report.agent_deadline_ms, report.agent_deadline_source),
                ),
                (
                    "verify",
                    deadline_member(report.verifier_deadline_ms, report.verifier_deadline_source),
                ),
            ]),
        ),
        ("classification", Json::str(&v.classification)),
        (
            "reward_ppm",
            v.reward_ppm
                .map(|p| Json::Int(p as i64))
                .unwrap_or(Json::Null),
        ),
        (
            "denied_hosts",
            Json::Arr(denied.iter().map(|h| Json::str(h.clone())).collect()),
        ),
    ]);
    let _ = write_json(&slot.join("cell-report.json"), &rep);
    Ok((passed, files, rows, denied))
}

fn h_run(
    cfg: &Cfg,
    task: &str,
    profile: &str,
    r: u32,
    work: &Path,
) -> Result<SurfaceOutcome, String> {
    let t0 = Instant::now();
    let mediated = profile == "allow";
    let mut o = SurfaceOutcome {
        rewards: vec![],
        digests: BTreeMap::new(),
        denied_hosts: vec![],
        harbor_exception: None,
        wall_ms: 0,
    };
    o.digests.insert(
        "image".to_string(),
        task_image(cfg, task).unwrap_or_default(),
    );
    for i in 0..r {
        let slot = work.join(format!("r{i}"));
        let _ = std::fs::create_dir_all(&slot);
        match h_run_once(cfg, task, mediated, &slot, None) {
            Ok((passed, files, _rows, denied)) => {
                o.rewards.push(passed);
                for h in denied {
                    if !o.denied_hosts.contains(&h) {
                        o.denied_hosts.push(h);
                    }
                }
                for (kind, b) in [
                    (
                        "reward",
                        files.reward_json.as_ref().or(files.reward_txt.as_ref()),
                    ),
                    ("ctrf", files.ctrf_json.as_ref()),
                    ("stdout", files.test_stdout.as_ref()),
                ] {
                    if let Some(b) = b {
                        o.digests
                            .insert(kind.to_string(), format!("sha256:{}", sha256_hex(b)));
                    }
                }
            }
            Err(e) => {
                o.rewards.push(false);
                if o.harbor_exception.is_none() {
                    o.harbor_exception = Some(format!("machinery:{e}"));
                }
            }
        }
    }
    o.wall_ms = t0.elapsed().as_millis() as u64;
    Ok(o)
}

// ── the hosted-arm cell drive (Ws1 PP0/corpus/L1; R6.45q) ────────────────
//
// `arm_run_once` — one hosted-arm slot in a mediated cell. The drive is
// `h_run_once`'s mediated topology verbatim (admit_allowlist_mediated →
// sealed phase layers → egress-mediator spawn → the relay pair →
// run_cell + deadlines + collect_denied), with the hosted differences:
// the agent argv/env/files come from `render::render` (RenderUse::
// HostedArm — the closed env carries the slot token, never argv), the
// image is the arm's layer image (`--image`, the R6.32b carried-export
// digest loaded at LOAD), `oracle_participant` is false (no /solution
// copy-in), the `/logs/agent` ST4 copy-out captures the trajectory, and
// the host-side `model.sock` is answered by a unix→TCP forwarder to the
// session proxy's `--model-upstream` loopback listen (the Wc stub's
// shape, piping) — never a dead socket.
//
// The rendered env is the agent phase's environment verbatim; the
// verify phase gets only the proxy-routing subset (the slot token never
// enters a verifier environment). Egress binds the declared
// `EGRESS_RELAY_PORT` (3128) — `closed_env`'s proxy hints are
// authoritative.

/// The slot-side parameters of one hosted-arm run (the session grant's
/// members the render folds into `SlotInputs` + the upstream wiring).
struct ArmSlot<'a> {
    slot_id: &'a str,
    slot_token: &'a str,
    model_upstream_port: u16,
    slot_slice_micros: u64,
}

fn arm_run_once(
    cfg: &Cfg,
    task: &str,
    image: &str,
    admission: &Json,
    slot: &Path,
    arm: &ArmSlot<'_>,
) -> Result<(Json, Vec<String>), String> {
    let staging = stage_task(cfg, task, slot)?;
    let collect = slot.join("collect");
    let work = slot.join("work");
    let _ = std::fs::create_dir_all(&work);
    let instruction = std::fs::read(Path::new(&cfg.tasks_dir).join(task).join("instruction.md"))
        .map_err(|e| format!("instruction: {e}"))?;
    let home = image_env(image, "HOME").unwrap_or_else(|| "/root".to_string());
    let user = image_env(image, "USER").unwrap_or_else(|| "root".to_string());
    let decl = hharness_runner::task_toml::load_task_decl(
        &std::path::Path::new(&cfg.tasks_dir).join(task),
    )?;
    let (agent_ms, agent_src) = hharness_runner::task_toml::resolve_deadline_ms(
        decl.agent_timeout_sec,
        cfg.agent_deadline_ms,
    );
    let (verify_ms, verify_src) = hharness_runner::task_toml::resolve_deadline_ms(
        decl.verifier_timeout_sec,
        cfg.verify_deadline_ms,
    );
    let slot_inputs = hharness_runner::render::SlotInputs {
        slot_token: arm.slot_token.to_string(),
        model_port: cfg.model_port,
        instruction: instruction.clone(),
        home,
        user,
        agent_wall_timeout_s: agent_ms / 1000,
        proxy_budget_ceiling_usd_micros: arm.slot_slice_micros,
        slot_id: arm.slot_id.to_string(),
    };
    let rc = hharness_runner::render::render(
        admission,
        &slot_inputs,
        hharness_runner::render::RenderUse::HostedArm,
    )
    .map_err(|e| e.to_string())?;
    let render_rec = hharness_runner::render::rendered_record(&rc, admission, &slot_inputs)
        .map_err(|e| e.to_string())?;
    let _ = write_json(&slot.join("hh-harness-render.json"), &render_rec);

    let agent_argv = rc.argv.clone();
    let verifier_argv = vec!["bash".to_string(), "/tests/test.sh".to_string()];

    // Mediated topology — verbatim from h_run_once's mediated arm.
    let mut rec_env = EnvironmentRecord {
        class: EnvironmentClass::LocalContainer,
        image: ImageRef::Tag(image.to_string()),
        build_context: None,
        platform: "linux/amd64".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("w1".to_string(), "v0".to_string()),
        limits: hharness_containment::policy::ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
        canary_channels: vec![],
    };
    let mut pol = hharness_containment::policy::kernel_default(0);
    let frz = freeze(cfg)?;
    let list = frz.list.clone();
    let model_sock = slot.join("model.sock");
    let egress_sock = slot.join("egress.sock");
    let cell_decl = MediatedCell {
        socket_mounts: BTreeMap::from([
            ("model".to_string(), model_sock.display().to_string()),
            ("egress".to_string(), egress_sock.display().to_string()),
        ]),
        relay_bin: cfg.relay_bin.clone(),
        pairs: vec![
            RelayPair {
                name: "model".to_string(),
                port: cfg.model_port,
            },
            RelayPair {
                name: "egress".to_string(),
                port: EGRESS_RELAY_PORT,
            },
        ],
    };
    let mdecl = MediatedDecl {
        family: cfg.family,
        allowlist: list,
        protocol_id: frz.protocol_id.clone(),
        epoch: frz.epoch,
        cell: cell_decl,
        model_port: cfg.model_port,
        slot_token: arm.slot_token.to_string(),
    };
    admit_allowlist_mediated(&mut rec_env, &mut pol, &mdecl, 0).map_err(|e| e.to_string())?;
    let sched = mediated_phase_schedule(&pol, &rec_env)?;
    let agent_policy = Some(phase_layer(&sched, "agent")?);
    let verify_policy = Some(phase_layer(&sched, "verify")?);
    let verify_version_id = verify_policy
        .as_ref()
        .and_then(|p| p.get("version_id"))
        .and_then(Json::as_str)
        .map(str::to_string);
    let policy_file = slot.join("policy.json");
    std::fs::write(
        &policy_file,
        agent_policy.as_ref().unwrap().to_canonical_string(),
    )
    .map_err(|e| e.to_string())?;
    let policy_dst = Some(policy_file.clone());
    // The host-side model socket answered by a unix→TCP forwarder to
    // the session proxy (the stub's socket-file contract, piping —
    // each accepted conn streams to 127.0.0.1:<upstream>).
    let fwd_script = format!(
        "import socket,threading\n\
         s=socket.socket(socket.AF_UNIX)\n\
         s.bind({:?})\n\
         s.listen(64)\n\
         def pump(a,b):\n\
         \ttry:\n\
         \t\twhile True:\n\
         \t\t\td=a.recv(65536)\n\
         \t\t\tif not d: break\n\
         \t\t\tb.sendall(d)\n\
         \texcept OSError: pass\n\
         \ttry: b.shutdown(socket.SHUT_WR)\n\
         \texcept OSError: pass\n\
         def serve(c):\n\
         \tu=socket.socket()\n\
         \tu.connect(('127.0.0.1',{}))\n\
         \tthreading.Thread(target=pump,args=(c,u),daemon=True).start()\n\
         \tpump(u,c)\n\
         while True:\n\
         \tc,_=s.accept()\n\
         \tthreading.Thread(target=serve,args=(c,),daemon=True).start()\n",
        model_sock.display().to_string(),
        arm.model_upstream_port
    );
    let _fwd = Command::new("python3")
        .args(["-c", &fwd_script])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let store_root = slot.join("mediator-store");
    let run_id = format!("ws1-{}", arm.slot_id);
    let med = Command::new(&cfg.mediator_bin)
        .args([
            "--store-root".to_string(),
            store_root.display().to_string(),
            "--socket".to_string(),
            egress_sock.display().to_string(),
            "--policy".to_string(),
            policy_file.display().to_string(),
            "--env-handle".to_string(),
            format!("env-{run_id}"),
            "--effect-id".to_string(),
            format!("eff-{run_id}"),
            "--participant".to_string(),
            arm.slot_id.to_string(),
            "--run-id".to_string(),
            run_id.clone(),
        ])
        .stdout(Stdio::from(
            std::fs::File::create(slot.join("mediator.log")).map_err(|e| e.to_string())?,
        ))
        .stderr(Stdio::from(
            std::fs::File::create(slot.join("mediator.err")).map_err(|e| e.to_string())?,
        ))
        .spawn()
        .map_err(|e| format!("mediator spawn: {e}"))?;
    let guard = MediatorGuard { child: med };
    let sock_deadline = Instant::now() + std::time::Duration::from_secs(10);
    while !egress_sock.exists() {
        if Instant::now() > sock_deadline {
            return Err(format!(
                "mediator egress socket {} never bound",
                egress_sock.display()
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    // The verify phase's env: the proxy-routing subset only — the
    // slot token and key channels never enter a verifier exec.
    let proxy = format!("http://127.0.0.1:{EGRESS_RELAY_PORT}");
    let verify_env: Vec<(String, String)> = vec![
        ("http_proxy".into(), proxy.clone()),
        ("https_proxy".into(), proxy.clone()),
        ("HTTP_PROXY".into(), proxy.clone()),
        ("HTTPS_PROXY".into(), proxy),
        ("no_proxy".into(), "127.0.0.1,localhost".into()),
        ("NO_PROXY".into(), "127.0.0.1,localhost".into()),
        ("HOME".into(), slot_inputs.home.clone()),
        ("USER".into(), slot_inputs.user.clone()),
    ];
    let backend = PodmanBackend {
        container: format!("hh-ws1-{}", arm.slot_id),
        image: image.to_string(),
        workspace: work.display().to_string(),
        network: false,
        extra_args: vec![],
        cell: CellSpec {
            workdir: None,
            cpus: decl.cpus.clone(),
            memory_mb: decl.memory_mb,
        },
        mounts: vec![
            SocketMount {
                name: "model".to_string(),
                host_path: model_sock.display().to_string(),
            },
            SocketMount {
                name: "egress".to_string(),
                host_path: egress_sock.display().to_string(),
            },
        ],
        relay: Some(RelaySpec {
            host_bin: cfg.relay_bin.clone(),
            pairs: vec![
                ("model".to_string(), cfg.model_port),
                ("egress".to_string(), EGRESS_RELAY_PORT),
            ],
        }),
    };
    backend.create().map_err(|e| e.to_string())?;
    backend.start().map_err(|e| e.to_string())?;
    // Slot-invariant rendered files + the adapter's setup exec land
    // before the agent phase (provisioning, never phase-owned paths).
    for f in &rc.files {
        if let Some(parent) = std::path::Path::new(&f.path).parent() {
            let _ = hharness_runner::cell::exec_guarded(
                &backend,
                &[
                    "/bin/mkdir".to_string(),
                    "-p".to_string(),
                    parent.to_string_lossy().to_string(),
                ],
                "/",
                &[],
                Some(60_000),
            )
            .map_err(|e| format!("render-file mkdir: {e}"))?;
        }
        let tmp = slot.join(format!("render-file-{}", f.path.replace('/', "_")));
        std::fs::write(&tmp, &f.bytes).map_err(|e| e.to_string())?;
        backend
            .cp_in(&tmp.display().to_string(), &f.path)
            .map_err(|e| e.to_string())?;
    }
    if let Some(sa) = &rc.setup_argv {
        let _ = hharness_runner::cell::exec_guarded(&backend, sa, "/", &[], Some(60_000))
            .map_err(|e| format!("setup exec: {e}"))?;
    }
    // The cell contract's `/logs/agent` (the ST4 copy-out source) is a
    // provisioning dir — the rendered argv writes trajectories into it.
    let _ = hharness_runner::cell::exec_guarded(
        &backend,
        &[
            "/bin/mkdir".to_string(),
            "-p".to_string(),
            "/logs/agent".to_string(),
        ],
        "/",
        &[],
        Some(60_000),
    )
    .map_err(|e| format!("logs-agent mkdir: {e}"))?;
    // Agent PATH — the harness bundle's `bin/` (through the declared
    // link) plus the node's runtime dir for arms that declare
    // `runtime.node` (pi, claude-code): the admission pins the version,
    // provisioning materializes it from the host-side dist tree.
    let mut path_parts = vec![
        "/usr/local/bin".to_string(),
        "/opt/hh-harness-link/bin".to_string(),
    ];
    let runtime = admission.get("runtime");
    let mkdir = |p: &str| {
        hharness_runner::cell::exec_guarded(
            &backend,
            &["/bin/mkdir".to_string(), "-p".to_string(), p.to_string()],
            "/",
            &[],
            Some(60_000),
        )
    };
    if runtime.and_then(|r| r.get("python")).is_some() {
        // The uv-tool bundle shebangs to its build-time managed python —
        // the dist tree is materialized where `bin/python` points.
        let _ = mkdir("/opt/python").map_err(|e| format!("python mkdir: {e}"))?;
        backend
            .cp_in(&format!("{}/.", cfg.python_dist_dir), "/opt/python")
            .map_err(|e| format!("python cp_in: {e}"))?;
        // `install.version_probe` names the entry (`bin/<arm>`); the
        // venv script shebangs to the uv tools dir — link it to the
        // bundle so the interpreter resolves.
        if let Some(name) = admission
            .get("install")
            .and_then(|i| i.get("version_probe"))
            .and_then(Json::as_arr)
            .and_then(|a| a.first())
            .and_then(Json::as_str)
            .and_then(|p| p.rsplit('/').next().map(str::to_string))
        {
            let script = format!(
                "D=$(dirname \"$(dirname \"$(readlink /opt/hh-harness-link/bin/python)\")\") && \
                 mkdir -p \"$D\" && cp -a /opt/python/. \"$D/\" && \
                 T=$(head -1 /opt/hh-harness-link/bin/{name} | tr -d '#!') && \
                 L=$(dirname \"$(dirname \"$T\")\") && \
                 mkdir -p \"$(dirname \"$L\")\" && \
                 rmdir \"$L\" 2>/dev/null; \
                 ln -sfnT /opt/hh-harness-link \"$L\""
            );
            let _ = hharness_runner::cell::exec_guarded(
                &backend,
                &["/bin/sh".to_string(), "-c".to_string(), script],
                "/",
                &[],
                Some(60_000),
            )
            .map_err(|e| format!("python link: {e}"))?;
        }
    }
    if runtime.and_then(|r| r.get("node")).is_some() {
        let _ = mkdir("/opt/node").map_err(|e| format!("node mkdir: {e}"))?;
        let _ = mkdir("/usr/local/bin").map_err(|e| format!("bin mkdir: {e}"))?;
        backend
            .cp_in(&format!("{}/.", cfg.node_dist_dir), "/opt/node")
            .map_err(|e| format!("node cp_in: {e}"))?;
        path_parts.insert(0, "/opt/node/bin".to_string());
        // `install.version_probe` names the entry the image is expected
        // to carry (`bin/<arm>`); the bundle lacks the shim, so mint it
        // at the probe path pointed at the package's declared bin entry.
        if let Some(probe_path) = admission
            .get("install")
            .and_then(|i| i.get("version_probe"))
            .and_then(Json::as_arr)
            .and_then(|a| a.first())
            .and_then(Json::as_str)
        {
            let script = format!(
                "P=$(find /opt/hh-harness-link/ -maxdepth 4 -name package.json \
                 -not -path '*/node_modules/*' | head -1) && \
                 f=$(/opt/node/bin/node -p \"(() => {{ \
                 const p = require('$P'); const b = p.bin || {{}}; \
                 const e = typeof b === 'string' ? b \
                 : Object.values(b)[0] || ''; \
                 const path = require('path'); \
                 return e ? path.resolve(path.dirname('$P'), e) : ''; \
                 }})()\") && \
                 mkdir -p \"$(dirname '{probe_path}')\" && \
                 printf '#!/bin/sh\\nexec /opt/node/bin/node \"%s\" \"$@\"\\n' \"$f\" \
                 > '{probe_path}' && chmod +x '{probe_path}'"
            );
            let _ = hharness_runner::cell::exec_guarded(
                &backend,
                &["/bin/sh".to_string(), "-c".to_string(), script],
                "/",
                &[],
                Some(60_000),
            )
            .map_err(|e| format!("bin shim: {e}"))?;
        }
    }
    let workdir = backend
        .image_workdir()
        .unwrap_or_else(|_| cfg.workdir_default.clone());
    let mut dc = DriveCell {
        backend: &backend,
        env: {
            let mut e: Vec<(String, String)> =
                rc.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            path_parts.extend([
                "/usr/local/sbin".to_string(),
                "/usr/local/bin".to_string(),
                "/usr/bin".to_string(),
                "/usr/sbin".to_string(),
                "/sbin".to_string(),
                "/bin".to_string(),
            ]);
            e.push(("PATH".to_string(), path_parts.join(":")));
            e
        },
        verify_env: Some(verify_env),
        _agent_argv: agent_argv.clone(),
        verifier_argv: verifier_argv.clone(),
        policy_dst,
        agent_policy,
        verify_policy,
    };
    let plan = CellPlan {
        oracle_participant: false,
        workdir,
        host_staging: staging,
        collect_dir: collect.clone(),
        agent_argv,
        agent_deadline_ms: Some(agent_ms),
        agent_deadline_source: agent_src,
        verifier_argv,
        verifier_deadline_ms: Some(verify_ms),
        verifier_deadline_source: verify_src,
        max_grade_attempts: cfg.max_grade_attempts,
        logs_copyout: Some(
            hharness_runner::capture::copyout_spec("/logs/agent", &slot.join("native-log"), &work)
                .map_err(|e| e.to_string())?,
        ),
    };
    let report = run_cell(&mut dc, &plan).map_err(|e| format!("run_cell: {e}"))?;
    let _ = backend.stop();
    drop(guard);
    let denied = collect_denied(&store_root, &run_id, verify_version_id.as_deref());
    let rep = Json::obj([
        (
            "agent_end_cause",
            Json::str(report.agent_end_cause.as_str()),
        ),
        ("verify_attempts", Json::Int(report.verify_attempts as i64)),
        (
            "native_log",
            report
                .native_log
                .as_ref()
                .map(|b| b.to_json())
                .unwrap_or(Json::Null),
        ),
        (
            "denied_hosts",
            Json::Arr(denied.iter().map(|h| Json::str(h.clone())).collect()),
        ),
    ]);
    let _ = write_json(&slot.join("cell-report.json"), &rep);
    Ok((rep, denied))
}

/// `image_env(image, name)` — the image-declared env member (podman
/// inspect → Config.Env `NAME=value`); the slot's HOME/USER source.
fn image_env(image: &str, name: &str) -> Option<String> {
    let out = Command::new("podman")
        .args(["image", "inspect", image])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let j = json::parse(&String::from_utf8_lossy(&out.stdout)).ok()?;
    let env = j.as_arr()?.first()?.get("Config")?.get("Env")?.as_arr()?;
    env.iter()
        .filter_map(Json::as_str)
        .find_map(|e| e.strip_prefix(&format!("{name}=")).map(str::to_string))
}

fn cmd_arm_run(args: &[String]) -> Result<String, String> {
    let cfg = load_cfg(&req(args, "--cfg"))?;
    let task = req(args, "--task");
    let image = req(args, "--image");
    let admission = read_json(&req(args, "--admission"))?;
    let slot_id = req(args, "--slot-id");
    let tok_file = req(args, "--slot-token-file");
    let slot_token = std::fs::read_to_string(&tok_file)
        .map_err(|e| format!("slot token: {e}"))?
        .trim()
        .to_string();
    let upstream: u16 = req(args, "--model-upstream")
        .parse()
        .map_err(|_| "--model-upstream int")?;
    let slice: u64 = arg(args, "--slot-slice-micros")
        .map(|s| s.parse::<u64>().unwrap_or(0))
        .unwrap_or(0);
    let out = PathBuf::from(req(args, "--out"));
    let _ = std::fs::create_dir_all(&out);
    let arm = ArmSlot {
        slot_id: &slot_id,
        slot_token: &slot_token,
        model_upstream_port: upstream,
        slot_slice_micros: slice,
    };
    let (rep, _denied) = arm_run_once(&cfg, &task, &image, &admission, &out, &arm)?;
    Ok(rep.to_canonical_string())
}

fn cmd_o_run(args: &[String]) -> Result<String, String> {
    let cfg = load_cfg(&req(args, "--cfg"))?;
    let task = req(args, "--task");
    let r: u32 = req(args, "--r").parse().map_err(|_| "r int")?;
    let out = PathBuf::from(req(args, "--out"));
    let _ = std::fs::create_dir_all(&out);
    let o = o_run(&cfg, &task, r, &out)?;
    let j = outcome_json(&o);
    write_json(&out.join("surface-outcome.json"), &j)?;
    Ok(j.to_canonical_string())
}

fn cmd_h_run(args: &[String]) -> Result<String, String> {
    let cfg = load_cfg(&req(args, "--cfg"))?;
    let task = req(args, "--task");
    let profile = req(args, "--profile");
    let r: u32 = req(args, "--r").parse().map_err(|_| "r int")?;
    let out = PathBuf::from(req(args, "--out"));
    let _ = std::fs::create_dir_all(&out);
    let o = h_run(&cfg, &task, &profile, r, &out)?;
    let j = outcome_json(&o);
    write_json(&out.join("surface-outcome.json"), &j)?;
    Ok(j.to_canonical_string())
}

// ── the prove pass ──────────────────────────────────────────────────────

struct VmDrive<'a> {
    cfg: &'a Cfg,
    run_root: PathBuf,
}

impl SurfaceDrive for VmDrive<'_> {
    fn run_surface(&mut self, surface: &str, task: &str, r: u32) -> SurfaceOutcome {
        let dir = self
            .run_root
            .join("trials")
            .join(task)
            .join(surface.replace('/', "_"));
        let _ = std::fs::create_dir_all(&dir);
        let o = match surface {
            "O" => o_run(self.cfg, task, r, &dir),
            "H-pub" => h_run(self.cfg, task, "pub", r, &dir),
            "H-allow" => h_run(self.cfg, task, "allow", r, &dir),
            _ => Err(format!("surface {surface}")),
        };
        match o {
            Ok(o) => {
                let _ = write_json(&dir.join("surface-outcome.json"), &outcome_json(&o));
                o
            }
            Err(e) => {
                let o = SurfaceOutcome {
                    rewards: (0..r).map(|_| false).collect(),
                    digests: BTreeMap::new(),
                    denied_hosts: vec![],
                    harbor_exception: Some(format!("drive:{e}")),
                    wall_ms: 0,
                };
                let _ = write_json(&dir.join("surface-outcome.json"), &outcome_json(&o));
                o
            }
        }
    }
}

fn cmd_prove(args: &[String]) -> Result<String, String> {
    let cfg = load_cfg(&req(args, "--cfg"))?;
    let tasks: Vec<String> = req(args, "--tasks")
        .split(',')
        .map(str::to_string)
        .collect();
    let r: u32 = req(args, "--r").parse().map_err(|_| "r int")?;
    let observe = match req(args, "--observe").as_str() {
        "h_allow_ask_to_log" => ObservePass::AskToLog,
        _ => ObservePass::MediatorRecordOnly,
    };
    let out_dir = PathBuf::from(req(args, "--out"));
    let _ = std::fs::create_dir_all(&out_dir);
    let plan = ReproofPlan {
        pin: pin(&cfg)?,
        r,
        observe,
        freeze: freeze(&cfg)?,
        tasks,
        fault_injection: None,
    };
    let mut drive = VmDrive {
        cfg: &cfg,
        run_root: PathBuf::from(&cfg.run_root),
    };
    let packet = reproof::prove(&plan, &mut drive).map_err(|e| e.to_string())?;
    let mut pj = packet.to_json();
    // the live-executed label is constructible only with the cited
    // hh-live-evidence/1 record — stamped here when the leg's record
    // already exists (Q-X39b answer (i); REQ-XH-027).
    if let Some(refr) = arg(args, "--live-evidence-ref") {
        if Path::new(&refr).is_file() {
            if let Json::Obj(m) = &mut pj {
                m.insert("evidence_label".into(), Json::str("live-executed"));
                m.insert("live_evidence_ref".into(), Json::str(&refr));
            }
        }
    }
    // strict-decode check on emission — a drifted packet refuses here.
    let _ = ReproofPacket::from_json(&pj).map_err(|e| e.to_string())?;
    write_json(&out_dir.join("reproof-packet.json"), &pj)?;
    write_json(
        &out_dir.join("reproof-register.json"),
        &packet.register.to_json(),
    )?;
    let admitted = packet
        .tasks
        .iter()
        .filter(|(_, (v, _))| v == "admit")
        .count();
    Ok(Json::obj([
        ("tasks", Json::Int(packet.tasks.len() as i64)),
        ("admitted", Json::Int(admitted as i64)),
        (
            "register_rows",
            Json::Int(packet.register.rows.len() as i64),
        ),
        ("evidence_label", Json::str(packet.evidence.as_str())),
    ])
    .to_canonical_string())
}

// ── REQ-XB-110 fault injection + REQ-XA-127 probe + census ──────────────

fn cmd_fault(args: &[String]) -> Result<String, String> {
    let cfg = load_cfg(&req(args, "--cfg"))?;
    let task = req(args, "--task");
    let out = PathBuf::from(req(args, "--out"));
    let _ = std::fs::create_dir_all(&out);
    // one mediated cell whose verify allowlist drops `astral.sh` —
    // the real mediator deny rows feed R6.9's classifier.
    let slot = out.join("fault-slot");
    let _ = std::fs::create_dir_all(&slot);
    let (_passed, files, rows, denied) = h_run_once(&cfg, &task, true, &slot, Some("astral.sh"))?;
    let observed = reproof::run_fault_injection(&files, &rows);
    let rec = Json::obj([
        ("schema", Json::str("hh-fault-injection/1")),
        ("leg", Json::str("REQ-XB-110")),
        ("task", Json::str(&task)),
        ("removed_host", Json::str("astral.sh")),
        (
            "expected",
            Json::str("oracle_failure{verifier_unavailable:bootstrap:egress_denied}"),
        ),
        ("observed", Json::str(&observed)),
        (
            "denied_hosts",
            Json::Arr(denied.iter().map(|h| Json::str(h.clone())).collect()),
        ),
    ]);
    write_json(&out.join("fault-injection.json"), &rec)?;
    Ok(rec.to_canonical_string())
}

fn cmd_probe_run(args: &[String]) -> Result<String, String> {
    let cfg = load_cfg(&req(args, "--cfg"))?;
    let task = req(args, "--task");
    let out = PathBuf::from(req(args, "--out"));
    let _ = std::fs::create_dir_all(&out);
    // the $0 probe: the task's oracle on every surface — the executed
    // record reports the real outcomes, never a verdict claim.
    let mut surfaces = BTreeMap::new();
    for s in ["O", "H-pub", "H-allow"] {
        let dir = out.join(s.replace('/', "_"));
        let _ = std::fs::create_dir_all(&dir);
        let o = match s {
            "O" => o_run(&cfg, &task, 1, &dir),
            "H-pub" => h_run(&cfg, &task, "pub", 1, &dir),
            _ => h_run(&cfg, &task, "allow", 1, &dir),
        };
        match o {
            Ok(o) => {
                let j = outcome_json(&o);
                let _ = write_json(&dir.join("surface-outcome.json"), &j);
                surfaces.insert(s.to_string(), j);
            }
            Err(e) => {
                surfaces.insert(s.to_string(), Json::str(format!("failed:{e}")));
            }
        }
    }
    let rec = Json::obj([
        ("schema", Json::str("hh-targeted-probe-result/1")),
        ("task", Json::str(&task)),
        ("surfaces", Json::Obj(surfaces)),
        ("cost_usd", Json::str("0")),
    ]);
    write_json(&out.join("probe-result.json"), &rec)?;
    Ok(rec.to_canonical_string())
}

fn cmd_census(args: &[String]) -> Result<String, String> {
    let manifest = read_json(&req(args, "--manifest"))?;
    let work = PathBuf::from(req(args, "--work"));
    let _ = std::fs::create_dir_all(&work);
    let entries = hharness_runner::l1_corpus::census_entries(&manifest)
        .map_err(|e| format!("census_entries: {}", e.detail))?;
    // drive each census sequence through the recorded-response
    // endpoint: the turn-1 upstream-bound request the real client
    // sends is the census fact (instruction bytes verbatim).
    let mut results = Vec::new();
    for e in &entries {
        let id = jstr(e, "task_id").unwrap_or("?").to_string();
        let rec_j = e.get("sequence").cloned().unwrap_or(Json::Null);
        let rec = Recording::parse(&rec_j).map_err(|e| format!("recording: {e}"))?;
        let req_body = rec
            .calls
            .first()
            .map(|c| c.downstream_body.clone())
            .unwrap_or_default();
        let replay = hharness_runner::l1_drive::SequenceReplay::new(rec.calls);
        let ep = hharness_runner::l1_drive::RecordedResponseEndpoint::bind(replay)
            .map_err(|e| e.detail)?;
        // the recorded turn-1 request re-posted through the real
        // socket — the endpoint captures what the client sends.
        let resp = {
            let req = format!(
                "POST /v1/chat/completions HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                req_body.len()
            );
            std::net::TcpStream::connect(format!("127.0.0.1:{}", ep.port())).and_then(|mut so| {
                use std::io::{Read as IoRead, Write as IoWrite2};
                IoWrite2::write_all(&mut so, req.as_bytes())
                    .and_then(|_| IoWrite2::write_all(&mut so, &req_body))?;
                let mut b = Vec::new();
                let _ = IoRead::read_to_end(&mut so, &mut b);
                Ok(b)
            })
        };
        let captured = ep.captured();
        results.push(Json::obj([
            ("task_id", Json::str(&id)),
            ("turns", Json::Int(captured.len() as i64)),
            (
                "turn1_upstream_body_sha256",
                captured
                    .first()
                    .map(|c| Json::str(format!("sha256:{}", sha256_hex(&c.upstream_body))))
                    .unwrap_or(Json::Null),
            ),
            ("responded", Json::Bool(resp.is_ok())),
        ]));
    }
    let out = Json::obj([
        ("schema", Json::str("hh-turn1-census/1")),
        ("entries", Json::Arr(results)),
        ("cost_usd", Json::str("0")),
    ]);
    write_json(Path::new(&req(args, "--out")), &out)?;
    Ok(out.to_canonical_string())
}

fn cmd_label_packet(args: &[String]) -> Result<String, String> {
    let pf = req(args, "--packet");
    let refr = req(args, "--ref");
    if !Path::new(&refr).is_file() {
        return Err(format!(
            "live evidence record {refr} absent — label stays honest"
        ));
    }
    let j = read_json(&pf)?;
    let mut packet = ReproofPacket::from_json(&j).map_err(|e| e.to_string())?;
    packet.evidence = EvidenceLabel::LiveExecuted(refr.clone());
    write_json(Path::new(&pf), &packet.to_json())?;
    Ok("live-executed".to_string())
}

fn cmd_index(args: &[String]) -> Result<String, String> {
    let af = req(args, "--artifacts");
    let entries_j = read_json(&af)?;
    let entries = entries_j
        .as_arr()
        .cloned()
        .ok_or_else(|| "artifacts must be an array".to_string())?;
    let doc = Json::obj([
        ("schema", Json::str("hh-receipt-index/1")),
        (
            "dataset",
            Json::obj([
                ("repo", Json::str("pending{w2_upload}")),
                ("revision", Json::Null),
                ("visibility", Json::str("private")),
            ]),
        ),
        ("attempts", Json::Arr(vec![])),
        ("artifacts", Json::Arr(entries)),
        (
            "scans",
            Json::obj([
                ("secret_scan_ref", Json::Null),
                ("contamination_scan_ref", Json::Null),
            ]),
        ),
    ]);
    hharness_runner::receipt_index::validate_index(&doc).map_err(|e| e.detail)?;
    write_json(Path::new(&req(args, "--out")), &doc)?;
    Ok("index: ok".to_string())
}

// ── l1-fold ───────────────────────────────────────────────────────
// The Ws1 L1 fold (R6.45q; spec §7.4 REQ-XD-103/104/109): the
// deterministic half of `l1_gate::run_sequence` composed over
// externally-supplied drives — the driver emits L1Drive JSON docs
// (`hh-l1-drive-report` captures + proxy receipts for O cells,
// `hh-l1-drive` events + receipts for H legs); this surface parses
// them, runs stable_prefix / compare_drives / diff_record /
// conformance_record verbatim, and emits `hh-l1-diff/1` per gated
// divergence + the `hh-l1-conformance/1` record bound to the exact H.
//
// spec.json shape:
//   {canon_id, surface, cell_facts:[[k,v]], corpus_digest,
//    h_id:{participant_ref,version,digest}, o_id:{…},
//    coverage:<Json>, census_covered:<int>, recorded_at,
//    seqs:[{seq_id, o_a:DRIVE, o_b:DRIVE, h1:DRIVE, h2:DRIVE,
//           endpoint_first_1:[turn,path]|null,
//           endpoint_first_2:[turn,path]|null}]}
// DRIVE := {requests:[{turn,method,path,content_type,body,upstream_body}],
//           parsed_actions:[[{command,tool_call_id}]],
//           executed_actions:[[…]], termination, submission,
//           extra_request, refusal:{rule,detail}|null}
fn l1_b64_decode(s: &str) -> Result<Vec<u8>, String> {
    fn v(c: u8) -> Result<u8, String> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            b'=' => Ok(0),
            _ => Err(format!("b64 char {c}")),
        }
    }
    let b: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(b.len() * 3 / 4);
    let mut i = 0;
    while i + 4 <= b.len() {
        let (a, bb, c, d) = (v(b[i])?, v(b[i + 1])?, v(b[i + 2])?, v(b[i + 3])?);
        out.push((a << 2) | (bb >> 4));
        if b[i + 2] != b'=' {
            out.push((bb << 4) | (c >> 2));
        }
        if b[i + 3] != b'=' {
            out.push((c << 6) | d);
        }
        i += 4;
    }
    Ok(out)
}

fn l1_body(j: &Json, plain: &str, b64: &str) -> Result<Vec<u8>, String> {
    if let Some(s) = j.get(b64).and_then(Json::as_str) {
        return l1_b64_decode(s);
    }
    Ok(j.get(plain)
        .and_then(Json::as_str)
        .unwrap_or_default()
        .as_bytes()
        .to_vec())
}

fn l1_actions(j: Option<&Json>) -> Vec<Vec<hharness_runner::l1_drive::L1Action>> {
    j.and_then(Json::as_arr)
        .map(|steps| {
            steps
                .iter()
                .map(|st| {
                    st.as_arr()
                        .map(|acts| {
                            acts.iter()
                                .map(|a| hharness_runner::l1_drive::L1Action {
                                    command: jstr(a, "command").unwrap_or_default().to_string(),
                                    tool_call_id: jstr(a, "tool_call_id")
                                        .unwrap_or_default()
                                        .to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn l1_drive_from_json(j: &Json) -> Result<hharness_runner::l1_drive::L1Drive, String> {
    let mut requests = Vec::new();
    for r in j
        .get("requests")
        .and_then(Json::as_arr)
        .cloned()
        .unwrap_or_default()
    {
        requests.push(hharness_runner::l1_drive::CapturedRequest {
            turn: r.get("turn").and_then(Json::as_int).unwrap_or(0) as usize,
            method: jstr(&r, "method").unwrap_or_default().to_string(),
            path: jstr(&r, "path").unwrap_or_default().to_string(),
            content_type: jstr(&r, "content_type").map(str::to_string),
            body: l1_body(&r, "body", "body_b64")?,
            upstream_body: l1_body(&r, "upstream_body", "upstream_body_b64")?,
        });
    }
    let refusal = j.get("refusal").and_then(|r| {
        r.as_obj().map(|_| hharness_runner::l1_obs::L1Error {
            rule: Box::leak(
                jstr(r, "rule")
                    .unwrap_or("drive")
                    .to_string()
                    .into_boxed_str(),
            ),
            detail: jstr(r, "detail").unwrap_or_default().to_string(),
        })
    });
    Ok(hharness_runner::l1_drive::L1Drive {
        requests,
        parsed_actions: l1_actions(j.get("parsed_actions")),
        executed_actions: l1_actions(j.get("executed_actions")),
        termination: jstr(j, "termination")
            .unwrap_or("exception{unspecified}")
            .to_string(),
        submission: jstr(j, "submission").map(str::to_string),
        extra_request: j
            .get("extra_request")
            .and_then(Json::as_bool)
            .unwrap_or(false),
        refusal,
    })
}

fn l1_identity_from_json(j: &Json) -> hharness_runner::l1_gate::L1Identity {
    hharness_runner::l1_gate::L1Identity {
        participant_ref: jstr(j, "participant_ref").unwrap_or_default().to_string(),
        version: jstr(j, "version").unwrap_or_default().to_string(),
        digest: jstr(j, "digest").unwrap_or_default().to_string(),
    }
}

fn cmd_l1_fold(args: &[String]) -> Result<String, String> {
    let spec_p = req(args, "--spec");
    let out = PathBuf::from(req(args, "--out"));
    let _ = std::fs::create_dir_all(&out);
    let spec = read_json(&spec_p)?;
    let canon_id = jstr(&spec, "canon_id")
        .unwrap_or("hh-l1-canon/1")
        .to_string();
    let surface = jstr(&spec, "surface").unwrap_or("openai-chat").to_string();
    let cell_facts: Vec<(String, String)> = spec
        .get("cell_facts")
        .and_then(Json::as_arr)
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    p.as_arr().and_then(|kv| {
                        Some((
                            kv.first()?.as_str()?.to_string(),
                            kv.get(1)?.as_str()?.to_string(),
                        ))
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let corpus_digest = jstr(&spec, "corpus_digest").unwrap_or_default().to_string();
    let h_id = l1_identity_from_json(&spec.get("h_id").cloned().unwrap_or(Json::Null));
    let o_id = l1_identity_from_json(&spec.get("o_id").cloned().unwrap_or(Json::Null));
    let coverage = spec.get("coverage").cloned().unwrap_or(Json::Null);
    let census_covered = spec
        .get("census_covered")
        .and_then(Json::as_int)
        .unwrap_or(0) as usize;
    let recorded_at = jstr(&spec, "recorded_at")
        .unwrap_or("2026-11-17T00:00:00Z")
        .to_string();
    let canon = || {
        hharness_gateway::replay::canonicalizer_for(&canon_id, &cell_facts)
            .map_err(|e| format!("canonicalizer: {e:?}"))
    };

    let mut seqs: Vec<hharness_runner::l1_gate::SeqOutcome> = Vec::new();
    let mut diff_docs: Vec<Json> = Vec::new();
    for seq in spec
        .get("seqs")
        .and_then(Json::as_arr)
        .cloned()
        .unwrap_or_default()
    {
        let seq_id = jstr(&seq, "seq_id").unwrap_or("seq").to_string();
        let drive_a = l1_drive_from_json(&seq.get("o_a").cloned().unwrap_or(Json::Null))?;
        let drive_b = l1_drive_from_json(&seq.get("o_b").cloned().unwrap_or(Json::Null))?;
        let h1 = l1_drive_from_json(&seq.get("h1").cloned().unwrap_or(Json::Null))?;
        let h2 = l1_drive_from_json(&seq.get("h2").cloned().unwrap_or(Json::Null))?;
        let c = canon()?;
        let s = hharness_runner::l1_gate::stable_prefix(c.as_ref(), &surface, &drive_a, &drive_b);
        let total = drive_a.requests.len().max(drive_b.requests.len());
        // L1.1 — whole-trajectory gate (usize::MAX), strict endpoint
        // first-divergence folded as a gated request-level divergence.
        let mut div1 = hharness_runner::l1_gate::compare_drives(
            c.as_ref(),
            &surface,
            &drive_a,
            &h1,
            usize::MAX,
            total,
        );
        let ep1 = seq
            .get("endpoint_first_1")
            .and_then(Json::as_arr)
            .and_then(|a| {
                Some((
                    a.first()?.as_int()? as usize,
                    a.get(1)?.as_str()?.to_string(),
                ))
            });
        if let Some((t, pth)) = &ep1 {
            if !div1
                .iter()
                .any(|d| d.turn == Some(*t) && d.level == "request")
            {
                div1.push(hharness_runner::l1_gate::L1Divergence {
                    level: "request",
                    turn: Some(*t),
                    path: pth.clone(),
                    detail: "strict endpoint refusal".to_string(),
                    gated: true,
                });
            }
        }
        let l1_1 = hharness_runner::l1_gate::LegOutcome {
            result: if div1.iter().any(|d| d.gated) {
                "fail"
            } else {
                "pass"
            },
            divergences: div1,
            endpoint_first: ep1,
            h_drive: Some(h1.clone()),
            o_drive: None,
        };
        // L1.2 — gated on requests 1…s(S).
        let div2 =
            hharness_runner::l1_gate::compare_drives(c.as_ref(), &surface, &drive_a, &h2, s, total);
        let ep2 = seq
            .get("endpoint_first_2")
            .and_then(Json::as_arr)
            .and_then(|a| {
                Some((
                    a.first()?.as_int()? as usize,
                    a.get(1)?.as_str()?.to_string(),
                ))
            });
        let l1_2 = hharness_runner::l1_gate::LegOutcome {
            result: if div2.iter().any(|d| d.gated) {
                "fail"
            } else {
                "pass"
            },
            divergences: div2,
            endpoint_first: ep2,
            h_drive: Some(h2.clone()),
            o_drive: None,
        };
        // diff records — one durable `hh-l1-diff/1` per gated divergence.
        let cmp = canon()?;
        for (leg, outcome, hd) in [("l1_1", &l1_1, &h1), ("l1_2", &l1_2, &h2)] {
            for d in outcome.divergences.iter().filter(|d| d.gated) {
                let cv = |dr: &hharness_runner::l1_drive::L1Drive| -> String {
                    d.turn
                        .and_then(|t| dr.requests.get(t - 1))
                        .and_then(|r| cmp.canonicalize(&surface, &r.upstream_body).ok())
                        .map(|cv| String::from_utf8_lossy(&cv.bytes).into_owned())
                        .unwrap_or_default()
                };
                diff_docs.push(hharness_runner::l1_gate::diff_record(
                    &seq_id,
                    leg,
                    d,
                    &canon_id,
                    &o_id,
                    &h_id,
                    &cv(&drive_a),
                    &cv(hd),
                    &recorded_at,
                ));
            }
        }
        seqs.push(hharness_runner::l1_gate::SeqOutcome {
            seq_id,
            stable: s,
            total,
            l1_1,
            l1_2,
            diffs: vec![],
        });
    }
    let seq_refs: Vec<&hharness_runner::l1_gate::SeqOutcome> = seqs.iter().collect();
    let conf = hharness_runner::l1_gate::conformance_record(
        &seq_refs,
        &coverage,
        census_covered,
        &canon_id,
        &corpus_digest,
        &h_id,
        &o_id,
        &recorded_at,
    );
    write_json(&out.join("hh-l1-conformance.json"), &conf)?;
    write_json(&out.join("hh-l1-diffs.json"), &Json::Arr(diff_docs.clone()))?;
    Ok(Json::obj([
        ("conformance", conf),
        ("diffs", Json::Int(diff_docs.len() as i64)),
    ])
    .to_canonical_string())
}

// ── l1-raws ───────────────────────────────────────────────────────
// Extract the recorded `RawObservation` queue from a corpus
// recording (R6.45q): L1.1's obs_replay executor is fed exactly this
// — the raw env effects embedded in the recorded tool messages.
// Emits [{output, returncode, exception_info}].
fn cmd_l1_raws(args: &[String]) -> Result<String, String> {
    let rec_path = req(args, "--recording");
    let out = PathBuf::from(req(args, "--out"));
    let rec_j = read_json(&rec_path)?;
    let rec = Recording::parse(&rec_j).map_err(|e| format!("recording: {e}"))?;
    let raws = hharness_runner::l1_obs::raws_from_recording(&rec)
        .map_err(|e| format!("raws: {}", e.detail))?;
    let arr: Vec<Json> = raws
        .iter()
        .map(|r| {
            Json::obj([
                ("output", Json::str(&r.output)),
                ("returncode", Json::Int(r.returncode)),
                (
                    "exception_info",
                    r.exception_info.clone().unwrap_or(Json::Null),
                ),
            ])
        })
        .collect();
    write_json(&out, &Json::Arr(arr.clone()))?;
    Ok(format!("raws: {}", arr.len()))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else { usage() };
    let rest = &args[1..];
    let r = match cmd.as_str() {
        "freeze" => cmd_freeze(rest),
        "task-images" => cmd_task_images(rest),
        "binary-set" => cmd_binary_set(rest),
        "image-export" => cmd_image_export(rest),
        "image-build" => cmd_image_build(rest),
        "observe" => cmd_observe(rest),
        "o-run" => cmd_o_run(rest),
        "h-run" => cmd_h_run(rest),
        "arm-run" => cmd_arm_run(rest),
        "prove" => cmd_prove(rest),
        "fault" => cmd_fault(rest),
        "census" => cmd_census(rest),
        "l1-fold" => cmd_l1_fold(rest),
        "l1-raws" => cmd_l1_raws(rest),
        "probe-run" => cmd_probe_run(rest),
        "label-packet" => cmd_label_packet(rest),
        "index" => cmd_index(rest),
        "render-policy" => cmd_render_policy(rest),
        "scan-hosts" => cmd_scan_hosts(rest),
        _ => {
            eprintln!("unknown subcommand {cmd}");
            usage()
        }
    };
    match r {
        Ok(s) => println!("{s}"),
        Err(e) => {
            eprintln!("hh-w1: {e}");
            std::process::exit(1);
        }
    }
}
