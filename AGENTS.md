# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Twip is a DOCX layout, editing and rendering engine in Rust, with hand-written SDKs for the web (wasm) and .NET (C ABI).
`.github/CONTRIBUTING.md` is the full contributor guide; this file is the short version plus agent rules.

## Principles

- Performance and security is first class. Avoid unnessasary allocations. And bound recursion and paths that could lead to DoS or RCE. DOCX inputs are untrusted. 
You can justify a small hit to performance if you can create a real DOCX that exercises the vulnerability. If it's only theoretical, and can't actually occur, document that in the code.
- Maintainability is important. Code is open source, so limit functions with high cognitive complexity unless it's truly complex or would hurt performance. This is up to your judgement,
and is a secondary consideration.
- Write idomatic and performant rust code

## Commands

**Always run Rust tests with nextest** (`cargo nextest run`), never `cargo test`. The one exception is doctests, which
nextest can't run: `cargo test --doc -p <crate>`.

```sh
cargo check -p twip --lib --tests                      # cheapest feedback while iterating
cargo nextest run -p twip --lib <module::filter>       # one test or module
cargo nextest run -p twip-graphics                     # the renderer's unit + golden tests
cargo clippy --workspace --all-targets -- -D warnings  # what root:lint runs
cargo fmt --all                                        # fixes root:format
```

Builds and tests dominate the time a change takes. While working, check and test only what the change touches; don't run
`--workspace` tests, the SDK suites (`web:test`, `dotnet:test`, `web:browser-test`), release builds or benchmarks
until the change is done. Then run the CI gate once: `pnpm moon ci` (tasks affected vs `main`) or a single task
with `pnpm moon run <project>:<task>` (task list in CONTRIBUTING, "Common tasks").

Setup: `pnpm install && pnpm moon setup` (installs toolchains and the commitlint hook). The web SDK needs
`cargo install wasm-bindgen-cli --version 0.2.129 --locked`. Toolchain is pinned in `rust-toolchain.toml`.

### Snapshot regeneration

Each of these rewrites checked-in expectations; review the diff before committing.

- Layout snapshots (`crates/twip/tests/layout`): `TWIP_WRITE_LAYOUT=1 cargo nextest run -p twip --lib layout_output`.
  Failures draw changed pages to `target/layout-diff/index.html`. A PR that changes them needs the `layout-change`
  label and a `fix(layout)` title saying what moves.
- Wire fixtures (`crates/twip/tests/wire`) and the wire schema (`crates/twip/tests/wire-schema.json`):
  `TWIP_WRITE_WIRE=1 cargo nextest run -p twip --lib fixtures_are_current` after changing any `twip::api` type, then
  the SDKs' generated types: `node scripts/wire.ts` in `bindings/web` (`src/wire.generated.ts`) and
  `node scripts/wire.mts` in `bindings/dotnet` (`Twip.Core/Wire.generated.cs`).
- Graphics goldens: `TWIP_GRAPHICS_BLESS=1 cargo nextest run -p twip-graphics --test golden`.
- SDK surface: `pnpm exec api-extractor run --local` in `bindings/web`; `.NET` uses `PublicAPI.Unshipped.txt`.

## Architecture

Pipeline: DOCX → document model (`crates/twip/src/docx`) → layout (`layout`) → immutable page fragments → drawing
(`render`, `drawing`) → `twip-graphics` (CPU reference backend, GPU via wgpu, PDF). Editing commands
(`editing`) mutate the model; layout follows incrementally.

- **Layout is LayoutNG-style.** Fragments (pages, boxes, paragraphs, lines) are immutable and positionless; the
  parent places them, so moved content is the same shared fragment. Each page starts from a break token; after an
  edit, layout restarts from the page before the change and stops once a page's start token matches the old
  layout, reusing every page after it. Untouched paragraphs keep cached shaping and line breaks.
- **Single-threaded core.** The engine is synchronous and uses `Rc`, not `Arc`; pointer identity is used for caching.
  Async exists only at the SDK boundary.
- **`twip::api` is the contract.** `crates/twip/src/api` defines every command, state field, event, error and warning.
  SDK methods are hand-written; most wire types are generated from the schema (`bindings/web/scripts/wire.ts` and
  `bindings/dotnet/scripts/wire.mts` list them, and the overrides for types each SDK shapes its own way). A new command means: `Command` variant + catalog entry + engine code, a wire fixture, a
  hand-written method in each SDK (`bindings/web/src/commands.ts`, .NET `EditorCommands`), both surface snapshots,
  and a step in a `tests/contract/*.json` script. All runners (Rust, web headless/worker, .NET) must agree;
  `tests/contract/pending.txt` lists scripts awaiting their feature (runners fail when a pending one passes).
- `twip::internal` re-exports engine modules for `twip-tools` and benches only; it is not API.
- `crates/twip-capi` is the C ABI the .NET SDK loads; `crates/twip-wasm` builds that ABI for the browser.
  `node crates/twip-capi/build-native.mjs [cargo args]` wraps cargo with CI's environment (needed on musl Linux;
  on Windows a bare run also fetches DXC for D3D12).

### twip-graphics rules (stricter than the engine)

`unsafe` forbidden; SIMD via `fearless_simd` with byte-identical output at every ISA level (integer pixel math, no
FMA, no `f32` reductions). No per-frame/line/glyph allocation once warm. GPU code uses only what WebGL 2 has (no
compute, storage buffers or push constants). Every drawing feature lands with a golden image; GPU output is checked
against the CPU's. No "this case is rare" shortcuts.

## Conventions

- **Comments name structure, they don't explain** (`//! Table autofit`, `// Row heights`). Exceptions: public API
  docs (`twip::api`, SDKs) and `// SAFETY:`. Every public member must be documented; the build enforces it.
- Don't ever put internal planning ids or references into comments
- Files are LF (`.gitattributes` `eol=lf`). Scripted edits on Windows must write LF (Python:
  `open(p, 'w', encoding='utf-8', newline='\n')`).
- History is linear: rebase, never merge. Branch from `main`.

## Commits

- Conventional commits, enforced by commitlint (`<type>(<scope>)!: <summary>`; scopes in `package.json`'s
  `commitlint` key). PRs are squash-merged, so the PR title becomes the `main` commit.
- Keep the subject short enough not to wrap in the GitHub UI (about 50 characters), with no body.
- **AI agents must never add themselves as co-authors** (no `Co-Authored-By:` trailer, no "Generated with" line) and
  **never put session links in a commit message or PR body.**
