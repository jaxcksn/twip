// Builds the native library the .NET and Swift SDKs load (target/release/twip.dll, libtwip.so or libtwip.dylib). It
// passes other cargo arguments through, with the same environment (CI's clippy and tests). On Windows the library build
// also puts DXC's dxcompiler.dll beside it: D3D12 compiles the GPU screen's shaders with it, and without it Windows
// draws with Vulkan. Usage: node build-native.mjs [cargo arguments]
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { inflateRawSync } from 'node:zlib';

// Linux with musl rather than glibc: Node reports no glibc version there.
const MUSL = process.platform === 'linux' && !process.report.getReport().header.glibcVersionRuntime;
const root = resolve(import.meta.dirname, '../..');
const env = { ...process.env };
// A cdylib on musl links the C runtime dynamically, as the .NET runtime that loads it does.
if (MUSL) env.RUSTFLAGS = [env.RUSTFLAGS, '-C target-feature=-crt-static'].filter(Boolean).join(' ');
const args = process.argv.slice(2);
// The library unwinds panics, which the release profile otherwise aborts on: its entry points catch them and report
// an internal error, so a panic on a bad document fails the call instead of ending the host's process.
const library = ['--config', 'profile.release.panic="unwind"', 'rustc', '-p', 'twip-capi', '--release', '--crate-type', 'cdylib'];
execFileSync('cargo', args.length ? args : library, { cwd: root, env, stdio: 'inherit' });
if (process.platform === 'win32' && !args.length) await dxc();

/** DXC's release, pinned: the zip's SHA-256 is GitHub's digest for the asset. Its licences are in licenses/dxc.txt. */
async function dxc() {
  const release = {
    url: 'https://github.com/microsoft/DirectXShaderCompiler/releases/download/v1.9.2609/dxc_2026_09_29.zip',
    sha256: 'ad31b1fc8443175d204f77a611fdb3ef2ec42759bdc2f1167368de24a4a7e7f1',
  };
  const cache = join(root, 'target', 'dxc', release.sha256.slice(0, 16));
  const zip = join(cache, 'dxc.zip');
  const digest = (file) => createHash('sha256').update(readFileSync(file)).digest('hex');
  if (!existsSync(zip) || digest(zip) !== release.sha256) {
    mkdirSync(cache, { recursive: true });
    const response = await fetch(release.url);
    if (!response.ok) throw new Error(`DXC: ${release.url} answered ${response.status}`);
    writeFileSync(zip, Buffer.from(await response.arrayBuffer()));
    if (digest(zip) !== release.sha256) {
      rmSync(zip);
      throw new Error(`DXC: ${release.url} isn't the pinned release (its SHA-256 differs)`);
    }
  }
  writeFileSync(join(root, 'target', 'release', 'dxcompiler.dll'), entry(readFileSync(zip), 'bin/x64/dxcompiler.dll'));
}

/** The file `name` from the zip in `zip`, from its central directory (the release's names use backslashes). */
function entry(zip, name) {
  const end = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (end < 0) throw new Error('DXC: the release is no zip');
  let at = zip.readUInt32LE(end + 16);
  for (let i = zip.readUInt16LE(end + 10); i > 0; i--) {
    const [method, size, nameLength] = [zip.readUInt16LE(at + 10), zip.readUInt32LE(at + 20), zip.readUInt16LE(at + 28)];
    const local = zip.readUInt32LE(at + 42);
    if (zip.toString('utf8', at + 46, at + 46 + nameLength).replaceAll('\\', '/') === name) {
      const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const data = zip.subarray(start, start + size);
      return method === 8 ? inflateRawSync(data) : data;
    }
    at += 46 + nameLength + zip.readUInt16LE(at + 30) + zip.readUInt16LE(at + 32);
  }
  throw new Error(`DXC: the release has no ${name}`);
}
