//! `hh-env` — the Stage-1 execution-environment abstraction and sandboxed
//! tool-execution / effect-capture pipeline (S1.16; R-2.2.5, R-2.5.5).
//!
//! The environment half (`record`/`handle`/`snapshot`/`driver`) owns the
//! `local_host`/`local_sandboxed` classes, the `EnvironmentRecord` identity
//! (image-digest content address), the `declared → ready → torn_down`
//! lifecycle, and the kernel-mediated `EnvHandle` ops (no side door — every
//! op requires the handle to be `ready` and the `hh-containment` report to
//! verify).
//!
//! The execution half (`tokens`/`capture`/`observe`/`deadline`/`executor`/
//! `local`/`protocol`/`dispatch`) owns the seven stages `resolve → authorize
//! → prepare → commit → execute → capture → observe` (`dispatch`), the
//! attribution-token mint (`tokens`), the capture manifest with the
//! redaction-on-capture-path rule (`capture`), the closed error taxonomy and
//! lifecycle mapping (`observe`), the in-process executor (`local`), and the
//! helper protocol schema/seam (`protocol` — the out-of-process helper is
//! S2.1).

pub mod capture;
pub mod deadline;
pub mod dispatch;
pub mod driver;
pub mod errors;
pub mod events;
pub mod executor;
pub mod handle;
pub mod local;
pub mod observe;
pub mod protocol;
pub mod record;
pub mod seal;
pub mod snapshot;
pub mod tokens;
