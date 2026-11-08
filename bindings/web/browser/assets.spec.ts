// Where the engine's files come from: beside the SDK's own code by default, or the folder `configure({ assets })`
// names, as for an application that bundles the SDK and serves dist/ elsewhere. The test server serves dist/ again at
// /twip/, and on the next port as another origin, where the worker starts from a same-origin bootstrap.
import { expect, test, type Page } from '@playwright/test';
import { mount, open, text } from './helpers.ts';

/** Every request URL the page and its worker make, and every console line. */
function watch(page: Page): { requests: string[]; console: string[] } {
  const seen = { requests: [] as string[], console: [] as string[] };
  page.context().on('request', (r) => seen.requests.push(r.url()));
  page.on('console', (m) => seen.console.push(m.text()));
  return seen;
}

/** Mounts an editor (in the worker) and a headless editor (on the main thread), and types in both. */
async function useBoth(page: Page): Promise<void> {
  await mount(page);
  expect(await page.evaluate(() => (window as any).editor.executionMode)).toBe('worker');
  await page.evaluate(() => (window as any).editor.commands.insertText('In the worker'));
  expect(await text(page)).toBe('In the worker');
  const headless = await page.evaluate(async () => {
    const editor = await (window as any).twip.HeadlessEditor.create();
    await editor.commands.insertText('On the main thread');
    return editor.getState().revision;
  });
  expect(headless).toBeGreaterThan(0);
}

test('by default, the engine loads from beside the SDK', async ({ page }) => {
  const seen = watch(page);
  await open(page);
  await useBoth(page);
  expect(page.workers().map((w) => new URL(w.url()).pathname)).toEqual(['/web/dist/worker.js']);
  expect(seen.requests.filter((u) => u.endsWith('.wasm')).map((u) => new URL(u).pathname)).toEqual(['/web/dist/twip_wasm.wasm', '/web/dist/twip_wasm.wasm']);
});

test('configure({ assets }) on the same origin: the worker and both engine loads come from there', async ({ page }) => {
  const seen = watch(page);
  await open(page, { assets: '/twip' });
  await useBoth(page);
  const origin = new URL(page.url()).origin;
  expect(page.workers().map((w) => w.url())).toEqual([`${origin}/twip/worker.js?assets=${encodeURIComponent(`${origin}/twip/`)}`]);
  expect(seen.requests.filter((u) => /twip[-_]wasm/.test(u))).toEqual(expect.arrayContaining([`${origin}/twip/twip-wasm.js`, `${origin}/twip/twip_wasm.wasm`]));
  expect(seen.requests.filter((u) => /\/web\/dist\/(worker\.js|twip-wasm\.js|twip_wasm\.wasm)/.test(u))).toEqual([]);
  // Too late to move it now.
  const code = await page.evaluate(() => {
    try {
      (window as any).twip.configure({ assets: '/elsewhere/' });
    } catch (e: any) {
      return e.code;
    }
  });
  expect(code).toBe('not-allowed');
});

test('configure({ assets }) on another origin: the worker starts from a same-origin bootstrap', async ({ page }) => {
  const seen = watch(page);
  // The browser asks before a page reaches a loopback address on another origin; a real CDN isn't on one.
  await page.context().grantPermissions(['local-network-access']);
  const cdn = `http://127.0.0.1:${Number(new URL(test.info().project.use.baseURL!).port) + 1}/twip/`;
  await open(page, { assets: cdn });
  await useBoth(page);
  expect(page.workers().map((w) => new URL(w.url()).protocol)).toEqual(['blob:']);
  expect(seen.requests).toEqual(expect.arrayContaining([`${cdn}worker.js?assets=${encodeURIComponent(cdn)}`, `${cdn}twip-wasm.js`, `${cdn}twip_wasm.wasm`]));
  // Compiled while streaming: the CDN's `application/wasm` survives CORS.
  expect(seen.console.filter((line) => line.includes('streaming compile failed'))).toEqual([]);
});
