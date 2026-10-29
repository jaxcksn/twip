// Builds the native library the .NET SDK loads (target/release/twip.dll, libtwip.so or libtwip.dylib). On Windows it
// first downloads the prebuilt Skia with Graphite on Dawn once (into .native-deps, not target/: CI's rust-cache
// deletes the files under target/ that aren't cargo's before saving), so the library draws on the GPU; elsewhere it
// is the plain cargo build. Other cargo commands get the same environment (CI's clippy and tests on Windows).
// Usage: node build-native.mjs [cargo arguments]
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, realpathSync, renameSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

// The prebuilt Windows Skia (a jaxcksn/skia-wasm-webgpu release from its build-windows workflow, built for
// skia-safe 0.153.3 with the features in crates/twip/Cargo.toml's native skia-safe line).
const SKIA = '0.153.3-windows-20ce3e1';

const root = resolve(import.meta.dirname, '../..');
const env = { ...process.env };
if (process.platform === 'win32') {
  mkdirSync(join(root, '.native-deps'), { recursive: true });
  const deps = realpathSync(join(root, '.native-deps'));
  const archive = join(deps, `skia-${SKIA}.tar.gz`);
  if (!existsSync(archive)) {
    const url = `https://github.com/jaxcksn/skia-wasm-webgpu/releases/download/${SKIA}/skia-binaries.tar.gz`;
    const res = await fetch(url);
    if (!res.ok) throw new Error(`${url}: ${res.status}`);
    writeFileSync(`${archive}.part`, Buffer.from(await res.arrayBuffer()));
    renameSync(`${archive}.part`, archive);
  }
  // skia-bindings unpacks the archive itself; the Dawn shim needs its headers and dawn_combined.lib.
  const unpacked = join(deps, `skia-${SKIA}`);
  if (!existsSync(join(unpacked, 'skia-binaries', 'dawn_combined.lib'))) {
    mkdirSync(unpacked, { recursive: true });
    // Windows' own tar: Git's GNU tar on PATH reads the D: of a path as a remote host.
    execFileSync(join(process.env.SystemRoot, 'System32', 'tar.exe'), ['-xzf', archive, '-C', unpacked], { stdio: 'inherit' });
  }
  // skia-bindings unpacks its archive by moving each entry up a level, which fails on a directory it moved before (a
  // rebuild into the same output): it gets the files alone.
  const slim = join(deps, `skia-${SKIA}-files.tar.gz`);
  if (!existsSync(slim)) {
    const dir = join(unpacked, 'skia-binaries');
    const files = readdirSync(dir).filter((f) => statSync(join(dir, f)).isFile()).map((f) => `skia-binaries/${f}`);
    execFileSync(join(process.env.SystemRoot, 'System32', 'tar.exe'), ['-czf', slim, '-C', unpacked, ...files], { stdio: 'inherit' });
  }
  env.SKIA_BINARIES_URL = `file://${slim.replaceAll('\\', '/')}`;
  env.TWIP_DAWN_DIR = join(unpacked, 'skia-binaries');
}
const args = process.argv.slice(2);
execFileSync('cargo', args.length ? args : ['rustc', '-p', 'twip-capi', '--release', '--crate-type', 'cdylib'], { cwd: root, env, stdio: 'inherit' });
