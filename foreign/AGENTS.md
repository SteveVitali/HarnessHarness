# AGENTS.md — foreign replay implementations

## Purpose

- `foreign/e2-python/` and `foreign/e3-typescript/` execute the committed cross-implementation corpus in Python and TypeScript. They are scoped replay candidates, not full runtime replacements.
- Apply the root `AGENTS.md` and preserve each implementation's README honesty notes and dated evidence.

## Key files

| File | Lines (approx) | Purpose |
|---|---|---|
| `crates/hh-xcheck/CHECKER.md` | ~200 | Bundle inputs, answer format, and comparison contract |
| `foreign/e2-python/foreign.py` | ~3.6k | Python standard-library replay implementation |
| `foreign/e3-typescript/foreign.ts` | ~50 | TypeScript replay entry point; arm implementations are in its `lib/` |
| `scripts/xcheck-foreign.sh` | ~150 | Regenerate answers and compare E1, E2, and E3 legs |

## Build & Test

- Run from the repository root. The combined replay needs the [Python prerequisite](e2-python/README.md#run) and a Node version meeting the [TypeScript runtime requirements](e3-typescript/README.md#toolchain).

```sh
cargo build -p hharness-xcheck
bash scripts/xcheck-foreign.sh
```

- For the TypeScript-only replay after building the checker: `bash scripts/xcheck-e3.sh`.
- The scripts generate fresh answers in temporary paths. Their default checker is `target/debug/hh-xcheck`; `HH_XCHECK_BIN` can select another built checker.
- Record the observed pass/fail/pending counts. The combined script exits 3 if a foreign runtime is missing; an absent runtime is not a successful comparison.

## Code conventions

| Correct ✓ | Wrong ✗ |
|---|---|
| Compute answers from the supplied corpus | Copy expected Rust answers or hardcode a successful verdict |
| Add source support and rerun the comparator | Edit committed answers to mask a mismatch |
| Describe unimplemented corpus/general-runtime scope explicitly | Claim a full independent kernel from fixture parity |

- Both implementations use their language standard libraries. The TypeScript tree runs without npm packages or a transpilation build; keep its syntax compatible with the documented runtime.
- Use `crates/hh-xcheck/CHECKER.md` for corpus/answer shapes and the comparator as the acceptance boundary. A changed corpus requires corresponding supported behavior, not a relaxed verifier.
- Committed `answers.json` files are evidence from recorded runs. Preserve prior evidence; updates need a new, traceable replay record under the repository's evidence protocol. Never represent an old answer file as a newly executed result.

## Critical gotchas

1. `foreign-run-verified` is its own evidence class. It does not raise the in-tree ceiling or establish live-service execution.
2. READMEs contain dated historical results. Correct a current instruction where allowed, but append to historical result accounts rather than silently rewriting what ran.
3. A comparator error is different from a failed case or missing answer. Preserve the scripts' exit statuses and reported classifications.
4. `foreign/e3-typescript/` exercises replay semantics; `clients/ts/` exercises the kernel client. They share a language, not an acceptance result.

## Terminology

| Term | Meaning |
|---|---|
| E1 / E2 / E3 | Rust reference / Python replay candidate / TypeScript replay candidate |
| Bundle arm | One family of inputs and answers in the committed cross-implementation corpus |

## Do

- Cite the exact corpus, implementation, toolchain, and newly generated comparison output when reporting replay evidence.

## Don't

- Do not replace recorded answers without provenance or imply broader coverage than the exercised corpus.
