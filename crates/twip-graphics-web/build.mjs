// Builds the browser comparisons into target/twip-graphics-web: the module (wasm32-unknown-unknown), its
// wasm-bindgen glue, and the page that runs it. Needs the wasm-bindgen CLI at the crate's wasm-bindgen version:
// `cargo install wasm-bindgen-cli --version 0.2.129 --locked`.
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..');
const target = process.env.CARGO_TARGET_DIR ?? join(root, 'target');
const out = join(target, 'twip-graphics-web');
execFileSync('cargo', ['build', '-p', 'twip-graphics-web', '--target', 'wasm32-unknown-unknown'], { cwd: root, stdio: 'inherit' });
mkdirSync(out, { recursive: true });
const wasm = join(target, 'wasm32-unknown-unknown', 'debug', 'twip_graphics_web.wasm');
execFileSync('wasm-bindgen', ['--target', 'web', '--no-typescript', '--out-dir', out, wasm], { stdio: 'inherit' });
copyFileSync(join(here, 'index.html'), join(out, 'index.html'));
