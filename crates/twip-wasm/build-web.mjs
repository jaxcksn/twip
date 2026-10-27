// Builds the WebAssembly module: downloads the prebuilt browser Skia and Dawn's emscripten port once (into
// .web-deps, not target/: CI's rust-cache deletes the files under target/ that aren't cargo's before saving), then
// runs the emscripten cargo build with them. Needs emsdk's emcc on PATH (CI: setup-emsdk; locally: emsdk_env).
// Usage: node build-web.mjs [--debug]
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';

// The prebuilt Skia (a jaxcksn/skia-wasm-webgpu release, built for skia-safe 0.153.3 with the features in
// crates/twip/Cargo.toml's emscripten skia-safe line), and the emsdk and Dawn releases it was built with: linking
// needs the same emsdk and emdawnwebgpu port. Change all three together with that repo's workflow pins.
const SKIA = '0.153.3-d85502f';
const DAWN = 'v20260831.205016';
const DAWN_SHA256 = '0c23f9dc1822b8eec25b864e1a355fe5b618b2af737a02d8eb230ec52625a6dd';

const root = resolve(import.meta.dirname, '../..');
const deps = join(root, '.web-deps');
const release = !process.argv.includes('--debug');
mkdirSync(deps, { recursive: true });

const download = async (url, to) => {
  if (existsSync(to)) return;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: ${res.status}`);
  writeFileSync(`${to}.part`, Buffer.from(await res.arrayBuffer()));
  renameSync(`${to}.part`, to);
};
const tar = (...args) => execFileSync('tar', args, { stdio: 'inherit' });

const skiaArchive = join(deps, `skia-wasm-${SKIA}.tar.gz`);
await download(`https://github.com/jaxcksn/skia-wasm-webgpu/releases/download/${SKIA}/skia-binaries.tar.gz`, skiaArchive);
// skia-bindings unpacks the archive itself; dawn.cpp needs its headers.
const skiaHeaders = join(deps, `skia-wasm-${SKIA}`);
if (!existsSync(join(skiaHeaders, 'skia-binaries', 'include'))) {
  mkdirSync(skiaHeaders, { recursive: true });
  tar('-xzf', skiaArchive, '-C', skiaHeaders);
}

const emdawn = join(deps, `emdawnwebgpu-${DAWN}`);
if (!existsSync(join(emdawn, 'emdawnwebgpu.port.py'))) {
  const zip = join(deps, `emdawnwebgpu_pkg-${DAWN}.zip`);
  await download(`https://github.com/google/dawn/releases/download/${DAWN}/emdawnwebgpu_pkg-${DAWN}.zip`, zip);
  if (createHash('sha256').update(readFileSync(zip)).digest('hex') !== DAWN_SHA256) {
    rmSync(zip);
    throw new Error(`emdawnwebgpu ${DAWN}: checksum mismatch`);
  }
  const tmp = join(deps, 'emdawn-tmp');
  rmSync(tmp, { recursive: true, force: true });
  mkdirSync(tmp);
  // Windows' and macOS' tar (bsdtar) read zip archives; GNU tar doesn't.
  if (process.platform === 'linux') execFileSync('unzip', ['-q', zip, '-d', tmp], { stdio: 'inherit' });
  else tar('-xf', zip, '-C', tmp);
  rmSync(emdawn, { recursive: true, force: true }); // an earlier unpack that didn't finish
  renameSync(join(tmp, 'emdawnwebgpu_pkg'), emdawn);
  rmSync(tmp, { recursive: true, force: true });
}

const exe = process.platform === 'win32' ? '.exe' : '';
const env = {
  ...process.env,
  EMDAWNWEBGPU_DIR: emdawn,
  SKIA_INCLUDE_DIR: join(skiaHeaders, 'skia-binaries'),
  SKIA_BINARIES_URL: `file://${skiaArchive.replaceAll('\\', '/')}`,
  CC_wasm32_unknown_emscripten: `emcc${exe}`,
  CXX_wasm32_unknown_emscripten: `em++${exe}`,
  AR_wasm32_unknown_emscripten: `emar${exe}`,
  // em++ links the C++ shim's standard library.
  CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER: `em++${exe}`,
};
const build = () =>
  execFileSync('cargo', ['build', '-p', 'twip-wasm', '--target', 'wasm32-unknown-emscripten', ...(release ? ['--release'] : [])], {
    cwd: root, env, encoding: 'utf8', stdio: ['inherit', 'inherit', 'pipe'],
  });
try {
  process.stderr.write(build() ?? '');
} catch (e) {
  process.stderr.write(e.stderr ?? '');
  // When its build script re-runs, skia-bindings unpacks the archive again and fails on the earlier unpack ("The
  // directory is not empty", then it attempts a source build). Clear that unpack and its fingerprint, then retry.
  if (!/directory is not empty/i.test(e.stderr ?? '')) throw new Error('web build failed');
  const out = join(root, 'target', 'wasm32-unknown-emscripten', release ? 'release' : 'debug');
  for (const d of readdirSync(join(out, 'build')).filter((d) => d.startsWith('skia-bindings-'))) {
    if (!existsSync(join(out, 'build', d, 'out', 'skia'))) continue;
    rmSync(join(out, 'build', d, 'out', 'skia'), { recursive: true, force: true });
    rmSync(join(out, '.fingerprint', d), { recursive: true, force: true });
  }
  console.error('Retrying after clearing a stale Skia unpack');
  build();
}
