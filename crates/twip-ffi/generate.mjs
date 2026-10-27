// Regenerates one SDK's bindings from src/lib.rs into an emptied folder, so removed types don't linger.
// Usage: node generate.mjs <dotnet|js> <out folder>
import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [lang, out] = process.argv.slice(2);
rmSync(out, { recursive: true, force: true });
execFileSync('diplomat-tool', ['-c', 'diplomat.toml', lang, out, '-s'], { stdio: 'inherit' });

// Diplomat 0.17's JS backend returns a function's `bool` result as the raw wasm integer (0 or 1), although its types
// say `boolean` (it converts only bools it reads from memory). Make each such method return a boolean.
if (lang === 'js') {
  let fixed = 0;
  for (const dts of readdirSync(out).filter((f) => f.endsWith('.d.ts'))) {
    const mjs = join(out, dts.replace(/\.d\.ts$/, '.mjs'));
    const names = [...readFileSync(join(out, dts), 'utf8').matchAll(/^\s+(?:static )?(?:get )?(\w+)\([^)]*\): boolean;$/gm)].map((m) => m[1]);
    if (!names.length) continue;
    let s = readFileSync(mjs, 'utf8');
    for (const name of names) {
      // The method's body: from its header to the next method header at the same indent.
      const re = new RegExp(`(\\n    (?:static )?(?:get )?${name}\\([^)]*\\) \\{(?:(?!\\n    \\}\\n)[\\s\\S])*?)return result;`, '');
      const before = s;
      s = s.replace(re, '$1return result === 1;');
      if (s !== before) fixed++;
    }
    writeFileSync(mjs, s);
  }
  console.log(`bool returns made boolean: ${fixed}`);

  // Byte-slice parameters accept typed arrays at runtime (the smoke test passes Uint8Array); say so in the types.
  let widened = 0;
  for (const dts of readdirSync(out).filter((f) => f.endsWith('.d.ts'))) {
    const file = join(out, dts);
    const s = readFileSync(file, 'utf8');
    const t = s.replace(/(\w+): Array<number>(?=[,)])/g, (_, name) => (widened++, `${name}: Uint8Array | Array<number>`));
    if (t !== s) writeFileSync(file, t);
  }
  console.log(`byte parameters typed Uint8Array | Array<number>: ${widened}`);
}
