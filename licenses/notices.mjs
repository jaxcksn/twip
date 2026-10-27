// Writes THIRD-PARTY-NOTICES.md: the Rust crates' licenses (cargo about) and the C/C++ libraries linked into the
// native library and the WebAssembly module (license texts in this folder). Usage: node licenses/notices.mjs <out>
import { execFileSync } from 'node:child_process';
import { readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const dir = import.meta.dirname;
const out = process.argv[2] ?? 'THIRD-PARTY-NOTICES.md';
// What the prebuilt Skia libraries contain (checked against their symbols); "both" = native and WebAssembly.
const native = [
  ['Skia', 'BSD-3-Clause', 'both', 'skia.txt'],
  ['libpng', 'libpng-2.0', 'both', 'libpng.txt'],
  ['libjpeg-turbo', 'IJG, BSD-3-Clause, Zlib', 'both', 'libjpeg-turbo.md'],
  ['Expat', 'MIT', 'both', 'expat.txt'],
  ['Wuffs', 'Apache-2.0 OR MIT', 'both', 'wuffs.txt'],
  ['zlib', 'Zlib', 'native', 'zlib.txt'],
  ['Vulkan Memory Allocator', 'MIT', 'native', 'vulkan-memory-allocator.txt'],
  ['FreeType', 'FTL', 'WebAssembly', 'freetype-ftl.txt'],
  ['Dawn (WebGPU C++ bindings)', 'BSD-3-Clause', 'WebAssembly', 'dawn.txt'],
  ['Emscripten WebGPU port', 'MIT OR NCSA', 'WebAssembly', 'emscripten-webgpu.txt'],
];
// cargo about refuses to write to a pipe on Windows, so it writes a file.
const tmp = `${out}.crates`;
execFileSync('cargo', ['about', 'generate', '--fail', '-c', join(dir, 'about.toml'), '-o', tmp, join(dir, 'about.hbs')], { stdio: 'inherit' });
const crates = readFileSync(tmp, 'utf8');
rmSync(tmp);
const libs = native.map(([name, id, where, file]) =>
  `### ${name} (${id})\n\nIn: ${where === 'both' ? 'the native library and the WebAssembly module' : `the ${where === 'native' ? 'native library' : 'WebAssembly module'}`}\n\n\`\`\`\n${readFileSync(join(dir, file), 'utf8').trim()}\n\`\`\`\n`);
writeFileSync(out, `${crates.trim()}\n\n## C and C++ libraries\n\n${libs.join('\n')}`);
console.log(`wrote ${out}`);
