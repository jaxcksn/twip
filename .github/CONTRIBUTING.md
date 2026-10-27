# Contributing to Twip

Thanks for helping. This guide covers setting up, making a change and getting it merged.

## Setup

You need:

- **Rust**: `rustup` installed. The repo pins the toolchain in `rust-toolchain.toml`; rustup picks it up on first build.
- **Node 24+** and **pnpm** (`corepack enable`).
- **.NET 10 SDK**: only for the .NET SDK.
- **emsdk 6.0.8**: only for the WebAssembly build. `emcc` must be on `PATH` (see below). Use exactly 6.0.8: the
  prebuilt browser Skia was built with it, and linking needs the same version.

Then:

```sh
pnpm install          # installs moon
pnpm moon setup       # installs the pinned toolchains and diplomat-tool, and the commit-msg hook
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
| `crates/twip` | The engine: layout, editing, rendering. This is the Rust API. |
| `crates/twip-ffi` | The binding surface, declared with [Diplomat](https://github.com/rust-diplomat/diplomat). |
| `crates/twip-native` | Builds the bridge as a shared library for .NET. |
| `crates/twip-wasm` | Builds the bridge as an emscripten module for the web. |
| `bindings/dotnet` | .NET SDK. The C# in `generated/` comes from `twip-ffi`. |
| `bindings/web` | TypeScript SDK. The JS in `generated/` comes from `twip-ffi`. |
| `packages/*` | Framework integrations (Angular, React). These version separately from the engine. |
| `docs/rulebook` | The layout rules. Code cites them by id. |

The SDKs are generated. Don't edit `bindings/*/generated`. Change `crates/twip-ffi/src/lib.rs` and regenerate.

## Common tasks

```sh
cargo test --workspace            # engine tests
pnpm moon run dotnet:test         # generate the .NET SDK, Native AOT publish, run the smoke test
pnpm moon run web:test            # generate the TypeScript SDK, build the wasm module, run the smoke test
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
```

## Layout rules and comments

Layout behaviour is specified in the rulebook (`docs/docs/rulebook`). Each rule has an id (for example `TABLE-9`).
Code that implements a rule cites it in a short comment (`// TABLE-9`) and doesn't explain the rule again. If you
change or add a behaviour, update the rule in the same pull request.

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

  Scopes are optional: `layout`, `editing`, `render`, `ffi`, `dotnet`, `web`, `angular`, `react`, `harness`, `docs`,
  `release`, `deps`. The list lives in the `commitlint` key of `package.json`.
  A change to layout output that alters where text lands on the page is a behaviour change. Mark it `fix(layout)` and
  say what moves.
- History is linear. No merge commits. Rebase on the target branch instead of merging it in.
- New behaviour needs a test. Layout changes need a fidelity check (see the harness docs).

## Releases

Every release comes from `main`. There are no release branches. Before 1.0, a fix ships in the next release and older
versions aren't patched. Maintainers can also publish a prerelease (`x.y.z-pre.N`) from `main` for testing.

## License

By contributing you agree that your contributions are licensed under the Apache License 2.0, as in `LICENSE`.
