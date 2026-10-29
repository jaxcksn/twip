// What the browser specs share: a page with the SDK, the document's text read from an export, and pixels read from
// screenshots (the canvas may be WebGPU or WebGL, which a 2D context can't read back).
import { readFileSync } from 'node:fs';
import { inflateRawSync } from 'node:zlib';
import { expect, type Locator, type Page } from '@playwright/test';

export const fixture = (name: string) => [...readFileSync(new URL(`../../../crates/twip/tests/fixtures/${name}`, import.meta.url))];
export const hello = [...readFileSync(new URL('../../hello.docx', import.meta.url))];

/** An empty page on the test server's origin, with the SDK as `window.twip` and a font loaded. */
export async function open(page: Page, options: { gpu?: boolean } = {}): Promise<void> {
  page.on('pageerror', (e) => console.log(`page error: ${e.message}`));
  await page.route('**/blank.html', (route) =>
    route.fulfill({ contentType: 'text/html', body: '<!doctype html><meta charset="utf-8"><body style="margin:0">' }),
  );
  await page.goto('/blank.html');
  await page.evaluate(async (gpu) => {
    // The CPU path, which every machine has: no WebGPU adapter and no WebGL 2.
    if (!gpu) {
      Object.defineProperty(navigator, 'gpu', { value: undefined });
      const get = HTMLCanvasElement.prototype.getContext;
      HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, kind: string, ...rest: unknown[]) {
        return kind === 'webgl2' ? null : get.call(this, kind as '2d', ...(rest as []));
      } as typeof get;
    }
    const twip = await import('/web/dist/index.js' as string);
    await twip.FontLibrary.default().add('/font.ttf');
    Object.assign(window, { twip });
  }, options.gpu ?? true);
}

/** Mounts an editor in a new `width` x `height` container, as `window[name]`. */
export async function mount(page: Page, options: { name?: string; width?: number; height?: number; docx?: number[]; mount?: object } = {}): Promise<void> {
  await page.evaluate(async ({ name, width, height, docx, mount }) => {
    const container = Object.assign(document.createElement('div'), { id: name });
    container.style.cssText = `width:${width}px;height:${height}px;`;
    document.body.append(container);
    const w = window as unknown as Record<string, any>;
    w[name] = await w.twip.Editor.mount(container, { ...(docx ? { document: new Uint8Array(docx) } : {}), ...mount });
  }, { name: options.name ?? 'editor', width: options.width ?? 640, height: options.height ?? 480, docx: options.docx, mount: options.mount ?? {} });
}

/** The editor's document as text, paragraphs on lines, from a DOCX export. */
export async function text(page: Page, name = 'editor'): Promise<string> {
  const bytes = await page.evaluate(async (name) => [...(await (window as any)[name].export({ format: 'docx' })).bytes], name);
  return docxText(Buffer.from(bytes));
}

/** `word/document.xml`'s text, from a DOCX's bytes. */
export function docxText(zip: Buffer): string {
  return docxXml(zip)
    .split(/<\/w:p>/)
    .map((p) => [...p.matchAll(/<w:t(?:\s[^>]*)?>([^<]*)<\/w:t>|<w:tab\/>/g)].map((m) => m[1] ?? '\t').join(''))
    .slice(0, -1)
    .join('\n')
    .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');
}

/** `word/document.xml`, from a DOCX's bytes. */
export function docxXml(zip: Buffer): string {
  for (let at = 0; at + 30 < zip.length; ) {
    if (zip.readUInt32LE(at) !== 0x04034b50) break;
    const [method, size, nameLength, extra] = [zip.readUInt16LE(at + 8), zip.readUInt32LE(at + 18), zip.readUInt16LE(at + 26), zip.readUInt16LE(at + 28)];
    const name = zip.toString('utf8', at + 30, at + 30 + nameLength);
    const start = at + 30 + nameLength + extra;
    if (name === 'word/document.xml') {
      const body = zip.subarray(start, start + size);
      return (method === 8 ? inflateRawSync(body) : body).toString('utf8');
    }
    at = start + size;
  }
  throw new Error('no word/document.xml (or a data descriptor in the way)');
}

/** The RGB of each pixel of a screenshot of `locator`, as a function of x and y (device px), and its size. */
export async function pixels(page: Page, locator: Locator): Promise<{ width: number; height: number; at(x: number, y: number): number }> {
  const png = await locator.screenshot();
  const { width, height, data } = await page.evaluate(async (b64) => {
    const bitmap = await createImageBitmap(await (await fetch(`data:image/png;base64,${b64}`)).blob());
    const c = new OffscreenCanvas(bitmap.width, bitmap.height).getContext('2d')!;
    c.drawImage(bitmap, 0, 0);
    // Base64 back: an array of every byte takes seconds to cross over.
    const bytes = c.getImageData(0, 0, bitmap.width, bitmap.height).data;
    let binary = '';
    for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    return { width: bitmap.width, height: bitmap.height, data: btoa(binary) };
  }, png.toString('base64'));
  const rgba = Buffer.from(data, 'base64');
  return { width, height, at: (x, y) => { const i = (Math.round(y) * width + Math.round(x)) * 4; return (rgba[i]! << 16) | (rgba[i + 1]! << 8) | rgba[i + 2]!; } };
}

/** How many distinct colours a screenshot of `locator` has: a drawn page has many, a blank frame one or two. */
export async function colours(page: Page, locator: Locator): Promise<number> {
  const p = await pixels(page, locator);
  const seen = new Set<number>();
  for (let y = 0; y < p.height; y += 2) for (let x = 0; x < p.width; x += 2) seen.add(p.at(x, y));
  return seen.size;
}

/** The SDK's live resources: DOM nodes, listeners, observers, timers, frames and workers. */
export function resources(page: Page): Promise<Record<string, number>> {
  return page.evaluate(() => (globalThis as any)[Symbol.for('twip.resources')]());
}

/** Waits until the editor's state passes `check`. */
export async function until(page: Page, check: string, name = 'editor'): Promise<void> {
  await expect.poll(() => page.evaluate(({ check, name }) => new Function('state', `return ${check}`)((window as any)[name].getState()), { check, name })).toBe(true);
}
