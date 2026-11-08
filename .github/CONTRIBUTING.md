# Contributing to Twip

Thanks for helping. This guide covers setting up, making a change and getting it merged.

## Setup

You need:

- **Rust**: `rustup` installed. The repo pins the toolchain in `rust-toolchain.toml`; rustup picks it up on first build.
- **Node 24+** and **pnpm** (`corepack enable`).
- **.NET 10 SDK**: for the .NET SDK and the docs' C# examples.
- **emsdk 6.0.8**: for the WebAssembly build, which `web:test` and everything else in `bindings/web` needs. `emcc`
  must be on `PATH` (see below). Use exactly 6.0.8: the prebuilt browser Skia was built with it, and linking needs
  the same version.

Skia is never built from source. Native builds use skia-safe's prebuilts, except on Windows and Apple silicon Macs:
`node crates/twip-capi/build-native.mjs` downloads the pinned Skia with Graphite on Dawn (a
[twip-skia-dawn](https://github.com/jaxcksn/twip-skia-dawn) release), so the native library draws on the GPU
(D3D12, D3D11 or Vulkan on Windows, Metal on macOS). It passes other cargo arguments through
(`node crates/twip-capi/build-native.mjs test --workspace`); a plain cargo build there draws with the CPU only.
On Linux musl (Alpine) the same script downloads the pinned musl Skia from twip-skia-dawn (built without Dawn, so it
draws with the CPU), because skia-safe publishes no musl prebuilts.

Then:

```sh
pnpm install          # installs moon
pnpm moon setup       # installs the pinned toolchains and the commit-msg hook
```

The commit-msg hook runs [commitlint](https://commitlint.js.org) (the `commitlint` key in `package.json`) on every commit, so a
message that isn't a conventional commit is rejected locally instead of in CI.

### Installing emsdk

macOS and Linux:

```sh
git clone https://github.com/emscripten-core/emsdk.git ~/emsdk
cd ~/emsdk
./emsdk install 6.0.8
./emsdk activate 6.0.8
source ./emsdk_env.sh   # in every new shell, or add it to your shell profile
```

Windows (PowerShell):

```powershell
git clone https://github.com/emscripten-core/emsdk.git $HOME\emsdk
cd $HOME\emsdk
.\emsdk install 6.0.8
.\emsdk activate 6.0.8 --permanent   # --permanent puts emcc on PATH for new shells
```

Without `--permanent`, run `.\emsdk_env.ps1` (PowerShell) or `emsdk_env.bat` (cmd) in each new shell instead.
Check it with `emcc --version`.

## Layout

| Path | What |
|---|---|
| `crates/twip` | The engine: layout, editing, rendering. `twip::api` is the contract every SDK follows. |
| `crates/twip-capi` | The C ABI over `twip::api` (internal, `include/twip.h`): the native library `twip` the .NET SDK loads. |
| `crates/twip-wasm` | Builds the C ABI as an emscripten module for the web, with `twip-host.js` for its callbacks. |
| `crates/twip-tools` | Developer tools: golden comparisons, dumps, round trips, renders (`cargo run -p twip-tools`). |
| `bindings/web` | `@twip/web`, the TypeScript SDK, written by hand over the wasm module. |
| `bindings/dotnet` | `Twip.Core` and `Twip.WinForms`, the .NET SDK, written by hand over the C ABI. |
| `tests/contract` | The contract scripts every SDK runs (see its README). |
| `packages/*` | Framework integrations (Angular, React). These version separately from the engine. |
| `docs` | The docs site: the guides, their examples (`docs/examples`) and the API references. |

## The API and the SDKs

`twip::api` (`crates/twip/src/api`) defines every operation, state field, event, error and warning. The SDKs are
not generated: each is written by hand in its language, and each follows the contract. Three things hold them to it:

- **Wire fixtures.** The api types serialize to the JSON the SDKs exchange with the engine. `crates/twip/tests/wire`
  holds one fixture per type; both SDKs round-trip every one through their own types, and `twip:test` fails when
  they differ from the api types. After changing an api type, run
  `TWIP_WRITE_WIRE=1 cargo test -p twip --lib fixtures_are_current` and commit the fixtures.
- **Contract scripts.** `tests/contract/*.json` are scripted scenarios (ordering, state, transactions, errors,
  services, review, cleanup) that run against the Rust API, `@twip/web` headless and in a worker, and `Twip.Core`
  unattached and attached; every runner must give the same results. A script in `tests/contract/pending.txt` waits
  for its feature: runners fail when it passes, so remove it from the list in the change that makes it pass.
  `python3 -I tests/contract/check.py` checks the scripts themselves: their steps, fixtures and command ids.
- **Surface snapshots.** A change to an SDK's public surface must show in its snapshot:
  `bindings/web/api/twip-web.api.md` (API Extractor; `pnpm exec api-extractor run --local` in `bindings/web`
  updates it) and `bindings/dotnet/*/PublicAPI.Unshipped.txt` (the analyzer's code fix adds the entries). CI fails
  on any difference.

So a new command, for example, is a `Command` variant with its catalog entry and engine code, its wire fixture, a
hand-written method in each SDK (`commands.ts`, `EditorCommands`), the two snapshots, and a contract script step.

Every public member is documented, and the build enforces it: `twip::api` denies missing docs, the .NET packages
fail on CS1591, and API Extractor fails on an undocumented export. The guides' examples live in `docs/examples` and
are compiled in CI (`moon run docs:examples`), so a surface change that breaks a guide fails there.

## Common tasks

Every check CI runs is a moon task, and CI runs them with `moon ci`. Run the same before you push:

```sh
pnpm moon ci                      # the tasks your branch's changes affect (against main), and those depending on them
pnpm moon ci --force              # every task
pnpm moon run twip:test           # one task: the engine's unit tests, the wire fixtures and the rust contract runner
pnpm moon run web:browser-test    # a mounted editor in a real browser
pnpm moon run docs:dev            # the docs site, locally
```

Builds dominate the time a change takes, so run the checks in order. While iterating, build and test only what the
change touches (`cargo check`, `cargo test -p twip <filter>`). Run `moon ci` once the change is done. Run the
benchmarks last, after everything else passes: `cargo bench -p twip [-- <filter>]` (Divan, `crates/twip/benches`;
not in CI). Compare their numbers with `main`'s on the same machine.

The tasks: `root:hygiene` (planning ids, the CI plan), `root:format`, `root:lint` (clippy), `root:contract-scripts`;
`twip:test` (the unit tests, including the wire fixture check, and the rust contract runner), `twip:example` (the
Rust API end to end), `twip-capi:test`; `web:test` (builds the wasm module with emsdk, then the unit, wire, contract and API report checks),
`web:browser-test`, `web:latency` (the latency budgets, on a quiet machine with a GPU; not in CI); `dotnet:build`, `dotnet:test` (builds twip-capi, then the wire, contract and Native AOT smoke
tests); `docs:build` and `docs:examples` (the guides' TypeScript and C# examples). `cargo fmt --all` fixes what
`root:format` reports.

## Comments

The code is the documentation. A comment in the engine names a part of the code (`//! Table autofit`,
`// Row heights`) and doesn't explain it: say what the code does through its names and types instead. Public API docs
(`twip::api`, the SDKs) and `// SAFETY:` comments are the exceptions.

## Commits and pull requests

- Branch from `main`.
- The PR title must be a [conventional commit](https://www.conventionalcommits.org/). PRs are squash-merged, so the
  title becomes the commit on `main` and drives the version and changelog. CI checks it.

  ```
  <type>(<scope>)!: <summary>
  ```

  | Type | Use for | Version effect (before 1.0) | After 1.0 |
  |---|---|---|---|
  | `feat` | New API or behaviour | patch | minor |
  | `fix` | Bug fix | patch | patch |
  | `perf` | Faster, same behaviour | patch | patch |
  | `feat!` / `fix!` or a `BREAKING CHANGE:` footer | Breaking API or behaviour change | minor | major |
  | `refactor`, `docs`, `test`, `build`, `ci`, `chore`, `revert` | Everything else | none | none |

  Scopes are optional: `layout`, `editing`, `drawing`, `render`, `pdf`, `ffi`, `dotnet`, `web`, `angular`, `react`, `harness`, `docs`,
  `release`, `deps`. The list lives in the `commitlint` key of `package.json`.
  A change to layout output that alters where text lands on the page is a behaviour change. Mark it `fix(layout)` and
  say what moves.
- History is linear. No merge commits. Rebase on the target branch instead of merging it in.
- New behaviour needs a test. Layout changes need a fidelity check (see the harness docs).

## Releases

Every release comes from `main`. There are no release branches. Before 1.0, a fix ships in the next release and older
versions aren't patched.

Every push to `main` also publishes a canary, `0.0.0-canary-<short SHA>`: `npm install @twip/web@canary`, and
`Twip.Core` and `Twip.WinForms` on GitHub Packages (`https://nuget.pkg.github.com/jaxcksn/index.json`, which needs a
GitHub token). A pull request labeled `preview` gets `@twip/web` on pkg.pr.new on every push, with the install command
in a comment. For NuGet, run the NuGet preview workflow from the branch and use the `nupkg` artifact as a local
source.

## License

By contributing you agree that your contributions are licensed under the Apache License 2.0, as in `LICENSE`.
