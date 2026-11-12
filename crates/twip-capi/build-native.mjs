// Builds the native library the .NET and Swift SDKs load (target/release/twip.dll, libtwip.so or libtwip.dylib). Its
// GPU frames are twip-graphics on wgpu, which needs nothing prebuilt; Skia (CPU frames while `skia-screen` stays, and
// PDF) does. On Windows it first downloads the prebuilt Skia once (into .native-deps, not target/: CI's rust-cache
// deletes the files under target/ that aren't cargo's before saving). On Linux it downloads the prebuilt Linux Skia the
// same way, because skia-safe publishes none with Skia's own FreeType (crates/twip/Cargo.toml's Linux skia-safe line).
// On macOS skia-bindings downloads rust-skia's Skia itself. Elsewhere it is the plain cargo build. Other cargo commands
// get the same environment (CI's clippy and tests on Windows).
// Usage: node build-native.mjs [cargo arguments]
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, realpathSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

// Linux with musl rather than glibc: Node reports no glibc version there.
const MUSL = process.platform === 'linux' && !process.report.getReport().header.glibcVersionRuntime;
// The prebuilt Skia, by platform (jaxcksn/twip-skia-dawn releases from its build-windows, build-linux and build-musl
// workflows, built for skia-safe 0.153.3 with the features in crates/twip/Cargo.toml's skia-safe lines). The Windows
// archive also carries Dawn, which nothing links now.
const SKIA = {
  'win32-x64': '0.153.3-windows-20ce3e1',
  'linux-x64': '0.153.3-linux-x86_64-30a1ae3',
  'linux-arm64': '0.153.3-linux-aarch64-30a1ae3',
  'linux-musl-x64': '0.153.3-musl-x86_64-30a1ae3',
  'linux-musl-arm64': '0.153.3-musl-aarch64-30a1ae3',
}[`${process.platform}${MUSL ? '-musl' : ''}-${process.arch}`];
// A file of the Windows archive's, there once it is unpacked.
const DAWN_LIB = 'dawn_combined.lib';
// Windows' own tar: Git's GNU tar on PATH reads the D: of a path as a remote host.
const TAR = process.platform === 'win32' ? join(process.env.SystemRoot, 'System32', 'tar.exe') : 'tar';

const root = resolve(import.meta.dirname, '../..');
// Puts a finished file in place. moon starts several cargo tasks at once and each prepares Skia: when another got there
// first (Windows refuses a rename onto a file another process has open), its file stays and this one goes.
const settle = (tmp, to) => {
  try {
    renameSync(tmp, to);
  } catch (e) {
    if (!existsSync(to)) throw e;
    rmSync(tmp, { force: true });
  }
};
const env = { ...process.env };
if (SKIA) {
  mkdirSync(join(root, '.native-deps'), { recursive: true });
  const deps = realpathSync(join(root, '.native-deps'));
  const archive = join(deps, `skia-${SKIA}.tar.gz`);
  if (!existsSync(archive)) {
    const url = `https://github.com/jaxcksn/twip-skia-dawn/releases/download/${SKIA}/skia-binaries.tar.gz`;
    const res = await fetch(url);
    if (!res.ok) throw new Error(`${url}: ${res.status}`);
    writeFileSync(`${archive}.${process.pid}`, Buffer.from(await res.arrayBuffer()));
    settle(`${archive}.${process.pid}`, archive);
  }
}
if (SKIA && process.platform === 'linux') {
  // Files only, no Dawn: skia-bindings unpacks it as it is.
  const archive = join(realpathSync(join(root, '.native-deps')), `skia-${SKIA}.tar.gz`);
  env.SKIA_BINARIES_URL = `file://${archive}`;
  // A cdylib on musl links the C runtime dynamically, as the .NET runtime that loads it does.
  if (MUSL) env.RUSTFLAGS = [env.RUSTFLAGS, '-C target-feature=-crt-static'].filter(Boolean).join(' ');
} else if (SKIA) {
  const deps = realpathSync(join(root, '.native-deps'));
  const archive = join(deps, `skia-${SKIA}.tar.gz`);
  // Unpacked here to repack its files alone (below).
  const unpacked = join(deps, `skia-${SKIA}`);
  if (!existsSync(join(unpacked, 'skia-binaries', DAWN_LIB))) {
    const tmp = `${unpacked}.${process.pid}`;
    mkdirSync(tmp, { recursive: true });
    execFileSync(TAR, ['-xzf', archive, '-C', tmp], { stdio: 'inherit' });
    // Into place whole. A rename fails onto a folder that exists: another task's finished unpack, which is kept, or an
    // earlier one that didn't finish, replaced.
    const done = () => existsSync(join(unpacked, 'skia-binaries', DAWN_LIB));
    try {
      renameSync(tmp, unpacked);
    } catch {
      if (!done()) {
        rmSync(unpacked, { recursive: true, force: true });
        renameSync(tmp, unpacked);
      }
    }
    rmSync(tmp, { recursive: true, force: true });
  }
  // skia-bindings unpacks its archive by moving each entry up a level, which fails on a directory it moved before (a
  // rebuild into the same output): it gets the files alone.
  const slim = join(deps, `skia-${SKIA}-files.tar.gz`);
  if (!existsSync(slim)) {
    const dir = join(unpacked, 'skia-binaries');
    const files = readdirSync(dir).filter((f) => statSync(join(dir, f)).isFile()).map((f) => `skia-binaries/${f}`);
    execFileSync(TAR, ['-czf', `${slim}.${process.pid}`, '-C', unpacked, ...files], { stdio: 'inherit' });
    settle(`${slim}.${process.pid}`, slim);
  }
  env.SKIA_BINARIES_URL = `file://${slim.replaceAll('\\', '/')}`;
}
const args = process.argv.slice(2);
// The library unwinds panics, which the release profile otherwise aborts on: its entry points catch them and report
// an internal error, so a panic on a bad document fails the call instead of ending the host's process.
const library = ['--config', 'profile.release.panic="unwind"', 'rustc', '-p', 'twip-capi', '--release', '--crate-type', 'cdylib'];
execFileSync('cargo', args.length ? args : library, { cwd: root, env, stdio: 'inherit' });
