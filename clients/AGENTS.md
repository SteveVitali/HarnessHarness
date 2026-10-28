# AGENTS.md — generated clients

## Purpose

- `clients/ts/` contains the TypeScript binding for `hh-embed/1` plus its handwritten smoke driver and fixture. Apply the root `AGENTS.md`.
- Keep client behavior aligned with the Rust schema source and loopback HTTP transport.

## Key files

| File | Lines (approx) | Purpose |
|---|---|---|
| `clients/ts/hh-embed.ts` | ~4.7k | Generated transport, operation types, and integrity checks |
| `clients/ts/smoke.ts` | ~150 | Handwritten end-to-end client exercise |
| `crates/hh-codegen/src/emit_ts.rs` | ~800 | TypeScript generator implementation |
| `scripts/e3-smoke.sh` | ~100 | Spawn a scratch kernel and run the client smoke |

## Build & Test

- Run at the repository root; Node must support direct execution of erasable TypeScript. See the [TypeScript runtime requirements](../foreign/e3-typescript/README.md#toolchain) for supported Node versions.

```sh
cargo build -p hharness-kernel
bash scripts/check-drift.sh
cargo test -p hharness-codegen --test r4_2
bash scripts/e3-smoke.sh
```

- The smoke launches a kernel on a free loopback port and uses temporary store/workspace paths. It exercises negotiation, open/submit/read, and event streaming.
- `scripts/e3-smoke.sh` exits 3 when its Node prerequisite is unavailable and 2 when the kernel binary is missing. Report either condition explicitly, not as a passed smoke.

## Code conventions

| Correct ✓ | Wrong ✗ |
|---|---|
| Change schema definitions in `crates/hh-embed-schema/`, then regenerate | Edit exported operation types directly in `hh-embed.ts` |
| Fix emitted client behavior in `crates/hh-codegen/src/emit_ts.rs` | Patch only the checked-in generated client |
| Keep handwritten checks in `clients/ts/smoke.ts` | Hide a mismatch by changing expected schema/version constants |

- The client uses Node standard-library transports. Do not add npm dependencies or a separate handwritten schema.
- Preserve the `node:http` transport and HTTP gate requirements. A replacement transport must satisfy the boundary's header policy; browser/fetch defaults can violate it.
- Keep schema hashes, negotiation, canonical encoding, and durable-event hash checks generated from their owning implementations.

## Critical gotchas

1. `clients/ts/hh-embed.ts` is generated; the smoke and `clients/ts/fixture-definition.json` are separate inputs, not generated client code.
2. Generator changes affect both the Rust and TypeScript outputs. Run regeneration and review every generated diff even when editing only one language.
3. The TypeScript client and `foreign/e3-typescript/` replay implementation have different roles. Passing one suite does not establish the other's behavior.

## Terminology

| Term | Meaning |
|---|---|
| Binding (c) | The kernel's authenticated loopback HTTP boundary |
| E3 client | TypeScript consumer of the kernel boundary |

## Do

- Verify the generated output and executable smoke together.

## Don't

- Do not commit service tokens or turn a missing Node runtime into fabricated evidence.
