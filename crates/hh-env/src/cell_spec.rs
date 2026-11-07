//! The declared cell spec (R6.9; AR-06; REQ-XB-107/XB-117) — the
//! admitted `ext["cell_spec"]` members a `local_container` record carries
//! into the helper spawn argv, plus the digest-only task-image admission
//! TB 2.1 slots enforce.
//!
//! - `CellSpec::from_record` reads `ext["cell_spec"]` strictly: a
//!   missing member yields `None` and the helper argv stays
//!   byte-identical to the pre-admission form; a malformed member
//!   refuses `Unsupported{cell_spec}` — a declared-but-unreadable spec
//!   is never silently dropped.
//! - `CellSpec::helper_args` maps the admitted members to the helper's
//!   closed flags (`--workdir`/`--workdir-from-image`, `--cpus`,
//!   `--memory`).
//! - `task_image_ref` admits a digest-pinned `name@sha256:<hex>` image
//!   as `ImageRef::ForeignDigest` recorded with platform and the R1
//!   reproducibility label; a tag-only reference refuses (REQ-XB-117).

use hharness_wire::json::Json;

use crate::errors::EnvError;
use crate::record::{EnvironmentRecord, ImageRef};

/// The `ext` member the declared cell spec lives under.
pub const SPEC_FIELD: &str = "cell_spec";
/// The reproducibility label a digest-pinned foreign image records
/// (F14: `ForeignDigest` caps at R0/R1/R3 — the task-image claim is R1).
pub const REPRO_R1: &str = "R1";

fn unsupported(detail: impl Into<String>) -> EnvError {
    EnvError::Unsupported {
        capability: "cell_spec",
        detail: detail.into(),
    }
}

/// `CellSpec` — the admitted per-cell members: the image WORKDIR
/// (`workdir_from_image` resolves it through the helper's
/// `podman image inspect`; `workdir` carries the already-resolved
/// value) and the task's declared `cpus`/`memory_mb` container limits.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CellSpec {
    /// `--workdir <path>` — a resolved WORKDIR carried verbatim.
    pub workdir: Option<String>,
    /// `--workdir-from-image` — the helper resolves
    /// `.Config.WorkingDir` at spawn (absent declaration refuses there).
    pub workdir_from_image: bool,
    /// `--cpus <n>` — the task's declared cpu count token.
    pub cpus: Option<String>,
    /// `--memory <MiB>` — the task's declared memory limit in MiB.
    pub memory_mb: Option<u64>,
}

impl CellSpec {
    /// `true` when nothing is declared — the spawn argv stays
    /// byte-identical to the pre-admission form.
    pub fn is_empty(&self) -> bool {
        self == &CellSpec::default()
    }

    /// `from_record(record) → Option<CellSpec>` — strict decode of
    /// `ext["cell_spec"]`. Absent ⇒ `None`; malformed ⇒ typed refusal
    /// (`workdir`/`workdir_from_image` are exclusive; `cpus` must be a
    /// positive number; `memory_mb` a positive integer).
    pub fn from_record(record: &EnvironmentRecord) -> Result<Option<CellSpec>, EnvError> {
        match record.ext.get(SPEC_FIELD) {
            None => Ok(None),
            Some(j) => Self::from_json(j).map(Some),
        }
    }

    /// `from_json(j)` — the strict member decode from_record wraps.
    pub fn from_json(j: &Json) -> Result<CellSpec, EnvError> {
        let Json::Obj(m) = j else {
            return Err(unsupported(format!("{SPEC_FIELD} must be an object")));
        };
        for k in m.keys() {
            if !["workdir", "workdir_from_image", "cpus", "memory_mb"].contains(&k.as_str()) {
                return Err(unsupported(format!("{SPEC_FIELD}: unknown member {k}")));
            }
        }
        let workdir = match m.get("workdir") {
            None => None,
            Some(Json::Str(w)) if !w.is_empty() => Some(w.clone()),
            Some(_) => return Err(unsupported("cell_spec.workdir must be a non-empty string")),
        };
        let workdir_from_image = match m.get("workdir_from_image") {
            None | Some(Json::Bool(false)) => false,
            Some(Json::Bool(true)) => true,
            Some(_) => return Err(unsupported("cell_spec.workdir_from_image must be a bool")),
        };
        if workdir.is_some() && workdir_from_image {
            return Err(unsupported(
                "cell_spec declares both workdir and workdir_from_image — \
                 they are exclusive",
            ));
        }
        let cpus = match m.get("cpus") {
            None => None,
            Some(Json::Int(n)) if *n > 0 => Some(n.to_string()),
            Some(Json::Str(s))
                if s.parse::<f64>()
                    .map(|f| f.is_finite() && f > 0.0)
                    .unwrap_or(false) =>
            {
                Some(s.clone())
            }
            Some(_) => return Err(unsupported("cell_spec.cpus must be a positive number")),
        };
        let memory_mb = match m.get("memory_mb") {
            None => None,
            Some(Json::Int(n)) if *n > 0 => Some(*n as u64),
            Some(_) => {
                return Err(unsupported(
                    "cell_spec.memory_mb must be a positive integer",
                ))
            }
        };
        Ok(CellSpec {
            workdir,
            workdir_from_image,
            cpus,
            memory_mb,
        })
    }

    /// `helper_args()` — the spawn-argv tail the admitted members map
    /// to, in a fixed order. An empty spec appends nothing (the argv
    /// stays byte-identical).
    pub fn helper_args(&self) -> Vec<String> {
        let mut a = Vec::new();
        if let Some(w) = &self.workdir {
            a.push("--workdir".to_string());
            a.push(w.clone());
        }
        if self.workdir_from_image {
            a.push("--workdir-from-image".to_string());
        }
        if let Some(c) = &self.cpus {
            a.push("--cpus".to_string());
            a.push(c.clone());
        }
        if let Some(m) = self.memory_mb {
            a.push("--memory".to_string());
            a.push(m.to_string());
        }
        a
    }

    /// The per-slot record echo (`cell.applied_limits` / the slot
    /// record's verbatim members).
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        if let Some(w) = &self.workdir {
            m.insert("workdir".to_string(), Json::str(w));
        }
        if self.workdir_from_image {
            m.insert("workdir_from_image".to_string(), Json::Bool(true));
        }
        if let Some(c) = &self.cpus {
            m.insert("cpus".to_string(), Json::str(c));
        }
        if let Some(mm) = self.memory_mb {
            m.insert("memory_mb".to_string(), Json::Int(mm as i64));
        }
        Json::Obj(m)
    }
}

/// `task_image_ref(reference, platform)` — the digest-only task-image
/// admission (REQ-XB-117): `name@sha256:<64-hex>` maps to
/// `ImageRef::ForeignDigest{scheme: "sha256", value: <full ref>,
/// source: "registry"}` and returns the per-slot record echo
/// `{image_ref, digest, platform, reproducibility: "R1"}`. A tag-only
/// reference (`name:tag`, a bare name, or `ref=latest`) refuses —
/// mutable selectors are never a pin.
pub fn task_image_ref(reference: &str, platform: &str) -> Result<(ImageRef, Json), EnvError> {
    let Some((name, digest)) = reference.split_once("@sha256:") else {
        return Err(EnvError::Unsupported {
            capability: "cell_spec.image_ref",
            detail: format!(
                "task image {reference} is not digest-pinned (expected \
                 <name>@sha256:<64-hex>) — a tag-only reference is a \
                 mutable selector, refused"
            ),
        });
    };
    if name.is_empty() || digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(EnvError::Unsupported {
            capability: "cell_spec.image_ref",
            detail: format!(
                "task image {reference} carries a malformed digest — \
                 expected <name>@sha256:<64-hex>"
            ),
        });
    }
    let image = ImageRef::ForeignDigest {
        scheme: "sha256".to_string(),
        value: reference.to_string(),
        source: "registry".to_string(),
    };
    let echo = Json::obj([
        ("image_ref", Json::str(reference)),
        ("digest", Json::str(format!("sha256:{digest}"))),
        ("platform", Json::str(platform)),
        ("reproducibility", Json::str(REPRO_R1)),
    ]);
    Ok((image, echo))
}
