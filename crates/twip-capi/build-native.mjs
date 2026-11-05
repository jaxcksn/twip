// Builds the native library the .NET SDK loads (target/release/twip.dll, libtwip.so or libtwip.dylib). On Windows and
// Apple silicon Macs it first downloads the prebuilt Skia with Graphite on Dawn once (into .native-deps, not target/:
// CI's rust-cache deletes the files under target/ that aren't cargo's before saving), so the library draws on the GPU.
// On Linux with musl (Alpine) it downloads the prebuilt musl Skia the same way (no Dawn there: it draws with the CPU),
// because skia-safe publishes none. Elsewhere it is the plain cargo build. Other cargo commands get the same
// environment (CI's clippy and tests on Windows).
// Usage: node build-native.mjs [cargo arguments]
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, realpathSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

// Linux with musl rather than glibc: Node reports no glibc version there.
const MUSL = process.platform === 'linux' && !process.report.getReport().header.glibcVersionRuntime;
// The prebuilt Skia, by platform (jaxcksn/twip-skia-dawn releases from its build-windows, build-macos and build-musl
// workflows, built for skia-safe 0.153.3 with the features in crates/twip/Cargo.toml's native skia-safe line). The
// Windows and macOS archives carry Dawn; the musl ones don't.
const SKIA = {
  'win32-x64': '0.153.3-windows-20ce3e1',
  'darwin-arm64': '0.153.3-macos-34047cc',
  'linux-musl-x64': '0.153.3-musl-x86_64-956ccc4',
  'linux-musl-arm64': '0.153.3-musl-aarch64-956ccc4',
}[`${process.platform}${MUSL ? '-musl' : ''}-${process.arch}`];
// Dawn's combined library in the archive.
const DAWN_LIB = process.platform === 'win32' ? 'dawn_combined.lib' : 'libdawn_combined.a';
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
if (SKIA && MUSL) {
  // Files only, no Dawn: skia-bindings unpacks it as it is.
  const archive = join(realpathSync(join(root, '.native-deps')), `skia-${SKIA}.tar.gz`);
  env.SKIA_BINARIES_URL = `file://${archive}`;
  // A cdylib on musl links the C runtime dynamically, as the .NET runtime that loads it does.
  env.RUSTFLAGS = [env.RUSTFLAGS, '-C target-feature=-crt-static'].filter(Boolean).join(' ');
} else if (SKIA) {
  const deps = realpathSync(join(root, '.native-deps'));
  const archive = join(deps, `skia-${SKIA}.tar.gz`);
  // skia-bindings unpacks the archive itself; the Dawn shim needs its headers and Dawn's library.
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
  env.TWIP_DAWN_DIR = join(unpacked, 'skia-binaries');
}
const args = process.argv.slice(2);
execFileSync('cargo', args.length ? args : ['rustc', '-p', 'twip-capi', '--release', '--crate-type', 'cdylib'], { cwd: root, env, stdio: 'inherit' });
