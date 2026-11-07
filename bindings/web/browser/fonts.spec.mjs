// Fonts in a real browser from a resolver on the page's own origin: settings made before the first editor (and, in
// worker mode, before the worker exists) apply, and a resolver's font, family and category answers draw within 2 s.
import { readFileSync } from 'node:fs';
import { expect, test } from '@playwright/test';

const fixtures = new URL('../../../crates/twip/tests/fixtures/editor/', import.meta.url);
const fixture = [...readFileSync(new URL('FX-font-families.docx', fixtures))];

test.beforeEach(async ({ page }) => {
  page.on('pageerror', (e) => console.log(`page error: ${e.message}`));
  // An empty page on the test server's origin, which the SDK loads from, and the stand-in fonts beside it.
  await page.route('**/blank.html', (route) => route.fulfill({ contentType: 'text/html', body: '<!doctype html><meta charset="utf-8">' }));
  await page.route('**/fonts/*.ttf', (route) =>
    route.fulfill({ contentType: 'font/ttf', body: readFileSync(new URL(`fonts/${new URL(route.request().url()).pathname.split('/').pop()}`, fixtures)) }),
  );
  await page.goto('/blank.html');
});

for (const execution of ['main', 'worker']) {
  test(`settings made before the editor apply, and resolver answers draw within 2 s (${execution})`, async ({ page }) => {
    const started = await page.evaluate(
      async ({ docx, execution }) => {
        const { Editor, FontLibrary } = await import('/web/dist/index.js');
        const fonts = FontLibrary.default();
        await fonts.add('/fonts/stand-in-serif.ttf');
        fonts.setAliases({ 'Times New Roman': 'Stand-in Serif' });
        fonts.setGenericDefaults({ serif: 'Stand-in Serif' });
        fonts.setResolver(async (request) =>
          request.family === 'Zzyzx Unknown' ? '/fonts/zzyzx-unknown.ttf' : request.family === 'Roboto Slab' ? { category: 'serif' } : null,
        );
        const container = Object.assign(document.createElement('div'), { style: 'width: 640px; height: 480px' });
        document.body.append(container);
        window.editor = await Editor.mount(container, { document: new Uint8Array(docx), execution });
        window.fonts = fonts;
        return performance.now();
      },
      { docx: fixture, execution },
    );
    await expect
      .poll(
        () =>
          page.evaluate(async () => {
            const { pending, missing } = window.editor.getState().fonts;
            return [pending.length, missing.map((m) => [m.family, m.by, m.drawnWith ?? null]), await window.fonts.families()];
          }),
        { timeout: 2000 },
      )
      .toEqual([0, [['Roboto Slab', 'generic', 'Stand-in Serif']], expect.arrayContaining(['Stand-in Serif', 'Zzyzx Unknown'])]);
    expect(await page.evaluate(() => performance.now())).toBeLessThan(started + 2000);
  });
}
