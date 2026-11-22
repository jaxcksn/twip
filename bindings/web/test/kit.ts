// The test font kit `crates/twip/tests/fonts/kit.json` lists: its faces and aliases, which the tests lay out with so
// results don't depend on the machine's fonts.
import { readFileSync } from 'node:fs';
import type { FontLibrary } from '../dist/index.js';

const dir = new URL('../../../crates/twip/tests/fonts/', import.meta.url);

/** Adds the kit's faces and aliases to `library`. */
export async function addKit(library: FontLibrary): Promise<void> {
  const kit = JSON.parse(readFileSync(new URL('kit.json', dir), 'utf8')) as { files: string[]; aliases: Record<string, string> };
  for (const file of kit.files) await library.add(readFileSync(new URL(file, dir)));
  library.setAliases(kit.aliases);
}
