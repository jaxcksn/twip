// Builds the WebAssembly module the TypeScript SDK loads into target/twip-wasm: the C ABI built for
// wasm32-unknown-unknown, then its JavaScript glue by wasm-bindgen (twip-wasm.js, and twip_wasm.wasm beside it). Needs
// the wasm-bindgen CLI at twip-capi's wasm-bindgen version: `cargo install wasm-bindgen-cli --version 0.2.129 --locked`.
// `test` runs the layout snapshot check (crates/twip/src/layout/tests/output.rs) in WebAssembly instead: built for
// wasm32-wasip1 and run under Node's WASI (wasi-test.mjs), against the snapshots twip:test compares natively.
// Usage: node build-web.mjs [--debug | test]
import { execFileSync } from 'node:child_process';
import { mkdirSync, renameSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '../..');
if (process.argv.includes('test')) {
  const env = { ...process.env, CARGO_TARGET_WASM32_WASIP1_RUNNER: `node ${join(import.meta.dirname, 'wasi-test.mjs')}` };
  execFileSync('cargo', ['test', '-p', 'twip', '--lib', '--target', 'wasm32-wasip1', 'layout_output'], { cwd: root, env, stdio: 'inherit' });
  process.exit(0);
}
const release = !process.argv.includes('--debug');
const target = process.env.CARGO_TARGET_DIR ?? join(root, 'target');
execFileSync('cargo', ['build', '-p', 'twip-wasm', '--target', 'wasm32-unknown-unknown', ...(release ? ['--release'] : [])], {
  cwd: root,
  stdio: 'inherit',
});
const out = join(target, 'twip-wasm');
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const wasm = join(target, 'wasm32-unknown-unknown', release ? 'release' : 'debug', 'twip_wasm.wasm');
execFileSync('wasm-bindgen', ['--target', 'web', '--no-typescript', '--out-dir', out, '--out-name', 'twip-wasm', wasm], { stdio: 'inherit' });
// The SDK serves the module as twip_wasm.wasm (its asset names).
renameSync(join(out, 'twip-wasm_bg.wasm'), join(out, 'twip_wasm.wasm'));
