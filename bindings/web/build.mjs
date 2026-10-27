// Assembles dist/: the generated bindings, the emscripten loader that replaces Diplomat's, and the engine module.
import { appendFileSync, cpSync, existsSync, mkdirSync, rmSync } from 'node:fs';

const release = '../../target/wasm32-unknown-emscripten/release';
rmSync('dist', { recursive: true, force: true });
mkdirSync('dist');
cpSync('generated', 'dist', { recursive: true });
cpSync('src/diplomat-wasm.mjs', 'dist/diplomat-wasm.mjs');
// The browser GPU helpers, exported beside the generated classes.
cpSync('src/gpu.mjs', 'dist/gpu.mjs');
cpSync('src/gpu.d.ts', 'dist/gpu.d.ts');
appendFileSync('dist/index.mjs', "export * from './gpu.mjs';\n");
appendFileSync('dist/index.d.ts', "export * from './gpu';\n");
cpSync(`${release}/twip-wasm.js`, 'dist/twip-wasm.js');
cpSync(`${release}/twip_wasm.wasm`, 'dist/twip_wasm.wasm');
// Generated at release time (licenses/notices.mjs).
if (existsSync('../../THIRD-PARTY-NOTICES.md')) cpSync('../../THIRD-PARTY-NOTICES.md', 'dist/THIRD-PARTY-NOTICES.md');
