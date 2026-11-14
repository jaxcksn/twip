// Runs a test binary built for wasm32-wasip1 under Node's WASI, with the workspace seen as /twip (the engine crate at
// /twip/crates/twip, which TWIP_ROOT tells the tests; target/ is where they write their diffs). Node's WASI on Windows
// can't list folders, so TWIP_FILES lists the files under the crate's tests/. Cargo calls it as the target's runner: node wasi-test.mjs <test.wasm> [args].
import { readdirSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { join, relative, resolve, sep } from 'node:path';
import { WASI } from 'node:wasi';

const [wasm, ...args] = process.argv.slice(2);
const workspace = resolve(import.meta.dirname, '../..');
const tests = join(workspace, 'crates', 'twip', 'tests');
const files = readdirSync(tests, { recursive: true, withFileTypes: true })
  .filter((e) => e.isFile())
  .map((e) => `/twip/crates/twip/tests/${relative(tests, join(e.parentPath, e.name)).split(sep).join('/')}`);
const wasi = new WASI({
  version: 'preview1',
  args: [wasm, ...args],
  env: { TWIP_ROOT: '/twip/crates/twip', TWIP_FILES: files.join('\n'), RUST_BACKTRACE: '1' },
  preopens: { '/twip': workspace },
  returnOnExit: true,
});
const module = await WebAssembly.compile(await readFile(wasm));
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
process.exitCode = wasi.start(instance);
