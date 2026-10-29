// Fonts from Google Fonts in a real browser, over a throttled network (10 Mbps down, 40 ms latency): the font
// fixture's families arrive within 2 s of mount.
import { readFileSync } from 'node:fs';
import { expect, test } from '@playwright/test';

const fixture = [...readFileSync(new URL('../../../crates/twip/tests/fixtures/editor/FX-font-families.docx', import.meta.url))];

test.beforeEach(async ({ page }) => {
  page.on('pageerror', (e) => console.log(`page error: ${e.message}`));
  // An empty page on the test server's origin, which the SDK loads from.
  await page.route('**/blank.html', (route) => route.fulfill({ contentType: 'text/html', body: '<!doctype html><meta charset="utf-8">' }));
  await page.goto('/blank.html');
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Network.emulateNetworkConditions', { offline: false, latency: 40, downloadThroughput: 10e6 / 8, uploadThroughput: 10e6 / 8 });
});

test('Google Fonts’ stylesheet and font files are readable from the page (CORS)', async ({ page }) => {
  const sizes = await page.evaluate(async () => {
    const css = await (await fetch('https://fonts.googleapis.com/css2?family=Tinos:ital,wght@0,400;0,700;1,400;1,700')).text();
    const url = /src:\s*url\(([^)]+)\)/.exec(css)?.[1];
    return [css.length, url ? (await (await fetch(url)).arrayBuffer()).byteLength : 0];
  });
  expect(sizes[0]).toBeGreaterThan(0);
  expect(sizes[1]).toBeGreaterThan(1000);
});

test('the font fixture’s families arrive within 2 s of mount', async ({ page }) => {
  const started = await page.evaluate(async (docx) => {
    const { Editor, FontLibrary } = await import('/web/dist/index.js');
    const fonts = FontLibrary.default();
    fonts.useGoogleFonts();
    fonts.substitute({ 'Times New Roman': 'Tinos' });
    const container = Object.assign(document.createElement('div'), { style: 'width: 640px; height: 480px' });
    document.body.append(container);
    window.editor = await Editor.mount(container, { document: new Uint8Array(docx) });
    window.fonts = fonts;
    return performance.now();
  }, fixture);
  await expect
    .poll(() => page.evaluate(async () => [window.editor.getState().fonts, await window.fonts.families()]), { timeout: 2000 })
    .toEqual([{ pending: [], unresolved: ['Zzyzx Unknown'] }, expect.arrayContaining(['Tinos', 'Roboto Slab'])]);
  expect(await page.evaluate(() => performance.now())).toBeLessThan(started + 2000);
});
