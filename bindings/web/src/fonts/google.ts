// Google Fonts as a font source: the css2 API's stylesheet for a family, then the font files it names.
import type { FaceInfo, FontSource } from './library.js';

/** The styles asked for, most first: a family without italics or weights fails the fuller requests. */
// shortcut: only regular, bold, italic and bold italic are requested
const styles = [':ital,wght@0,400;0,700;1,400;1,700', ':wght@400;700', ''];

/** A source that adds a family's faces from Google Fonts, each family fetched once; `families` limits it to those. */
export function googleFonts(add: (url: string, signal: AbortSignal) => Promise<FaceInfo[]>, families?: string[]): FontSource {
  const only = families && new Set(families.map((f) => f.toLowerCase()));
  const loads = new Map<string, Promise<boolean>>();
  return (request, signal) => {
    const key = request.family.toLowerCase();
    if (only && !only.has(key)) return Promise.resolve(false);
    let load = loads.get(key);
    if (!load) {
      loads.set(key, (load = fetchFamily(request.family, add, signal)));
      load.catch(() => loads.delete(key));
    }
    return load;
  };
}

/** Adds every face of `family` the stylesheet names; false when Google doesn't have the family. */
async function fetchFamily(family: string, add: (url: string, signal: AbortSignal) => Promise<FaceInfo[]>, signal: AbortSignal): Promise<boolean> {
  const name = encodeURIComponent(family).replace(/%20/g, '+');
  for (const style of styles) {
    // Google answers a family it doesn't have with a 400 that has no CORS header, so in a browser the fetch rejects:
    // a miss, not an error.
    const css = await fetch(`https://fonts.googleapis.com/css2?family=${name}${style}`, { signal }).catch((e: unknown) => {
      if (signal.aborted) throw e;
    });
    if (!css) return false;
    if (!css.ok) continue;
    const urls = [...(await css.text()).matchAll(/src:\s*url\((['"]?)([^'")]+)\1\)/g)].map((m) => m[2]!);
    const faces = await Promise.all([...new Set(urls)].map((url) => add(url, signal)));
    return faces.flat().length > 0;
  }
  return false;
}
