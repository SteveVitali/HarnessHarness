//! `CallerBinding` — the R-2.11.3⁰ caller-binding record. At Stage 3 the
//! only kind is `stdio_launch`, fixed to the **test principal** (R-3:
//! the binding is the launch, not a credential — claims never decide;
//! ADR-0174 D5). OAuth bindings through the H3 broker are a C1/Stage-4
//! kind and deliberately absent.

use hh_wire::json::Json;

/// The Stage-3 fixed principal — `stdio_launch` binds every caller to
/// it (the fixture's conformance identity; a second "client" is the
/// same principal by construction, which is what makes AC-R-2.11.3-1's
/// byte-identity meaningful).
pub const TEST_PRINCIPAL: &str = "principal:test";

/// The binding record's schema id.
pub const BINDING_SCHEMA: &str = "hh-caller-binding/1";

/// The `stdio_launch` CallerBinding record — `{schema, kind, principal,
/// issued_for}`; a record, never a credential (no secret, no token —
/// the launch *is* the binding).
pub fn stdio_launch_binding() -> Json {
    Json::obj([
        ("schema", Json::str(BINDING_SCHEMA)),
        ("kind", Json::str("stdio_launch")),
        ("principal", Json::str(TEST_PRINCIPAL)),
        ("issued_for", Json::str("fixture")),
    ])
}
