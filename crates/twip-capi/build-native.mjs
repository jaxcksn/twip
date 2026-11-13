// Builds the native library the .NET and Swift SDKs load (target/release/twip.dll, libtwip.so or libtwip.dylib). It
// passes other cargo arguments through, with the same environment (CI's clippy and tests).
// Usage: node build-native.mjs [cargo arguments]
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';

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
// On macOS its install name is @rpath/libtwip.dylib, so an app that embeds it finds it in its own Frameworks folder.
if (process.platform === 'darwin') library.push('--', '-C', 'link-arg=-Wl,-install_name,@rpath/libtwip.dylib');
execFileSync('cargo', args.length ? args : library, { cwd: root, env, stdio: 'inherit' });
