// Writes THIRD-PARTY-NOTICES.md: the Rust crates' licenses (cargo about) and the fonts and data built into the native
// library and the WebAssembly module (license texts in this folder). Usage: node licenses/notices.mjs <out>
import { execFileSync } from 'node:child_process';
import { readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const dir = import.meta.dirname;
const out = process.argv[2] ?? 'THIRD-PARTY-NOTICES.md';
// The fonts and data built into the native library and the WebAssembly module; "both" = both.
const native = [
  // The engine's last-resort symbol font, a subset of DejaVu Sans (font.rs).
  ['DejaVu Sans (subset)', 'Bitstream-Vera', 'both', 'dejavu-fonts.txt'],
  // The preset shape geometries (drawing/geom/presets.rs), generated from the standard's definitions.
  ['ECMA-376 preset shape definitions', 'LicenseRef-Ecma', 'both', 'ecma-376.txt'],
  // The East Asian punctuation font, a subset of Noto Serif JP with MS Mincho's widths (font.rs).
  ['Noto Serif JP (subset)', 'OFL-1.1', 'both', 'noto-serif-jp.txt'],
  // The editor chrome's typeface, subsets (render/chrome_fonts/).
  ['JetBrains Mono (subset)', 'OFL-1.1', 'both', 'jetbrains-mono.txt'],
  // The AutoCorrect replacement list (editing/autocorrect/en-US.tsv).
  ['LibreOffice AutoCorrect list (en-US)', 'MPL-2.0', 'both', 'libreoffice-autocorrect.txt'],
];
// cargo about refuses to write to a pipe on Windows, so it writes a file.
const tmp = `${out}.crates`;
execFileSync('cargo', ['about', 'generate', '--fail', '-c', join(dir, 'about.toml'), '-o', tmp, join(dir, 'about.hbs')], { stdio: 'inherit' });
const crates = readFileSync(tmp, 'utf8');
rmSync(tmp);
const libs = native.map(([name, id, where, file]) =>
  `### ${name} (${id})\n\nIn: ${where === 'both' ? 'the native library and the WebAssembly module' : `the ${where === 'native' ? 'native library' : 'WebAssembly module'}`}\n\n\`\`\`\n${readFileSync(join(dir, file), 'utf8').trim()}\n\`\`\`\n`);
writeFileSync(out, `${crates.trim()}\n\n## C and C++ libraries, fonts and data\n\n${libs.join('\n')}`);
console.log(`wrote ${out}`);
