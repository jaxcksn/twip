// Builds dist/: the SDK compiled by tsc (ESM and .d.ts), plus the engine module from the emscripten build.
import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, rmSync } from 'node:fs';

const release = '../../target/wasm32-unknown-emscripten/release';
rmSync('dist', { recursive: true, force: true });
execFileSync(process.execPath, ['node_modules/typescript/bin/tsc'], { stdio: 'inherit' });
// The engine module: loaded by dist/transport/module.js from dist/.
for (const file of ['twip-wasm.js', 'twip_wasm.wasm']) {
  if (existsSync(`${release}/${file}`)) cpSync(`${release}/${file}`, `dist/${file}`);
  else console.warn(`build: ${release}/${file} is missing (moon run twip-wasm:build); the package has no engine`);
}
// Generated at release time (licenses/notices.mjs).
if (existsSync('../../THIRD-PARTY-NOTICES.md')) cpSync('../../THIRD-PARTY-NOTICES.md', 'dist/THIRD-PARTY-NOTICES.md');
