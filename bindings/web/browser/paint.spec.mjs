// Paints hello.docx through each browser GPU path and checks the frame isn't blank (page, background and text
// make many colours; a failed paint leaves one or two).
import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  page.on('pageerror', (e) => console.log(`page error: ${e.message}`));
  await page.goto('/web/browser/index.html');
  await page.waitForFunction(() => window.ready === true);
});

test('WebGL 2 paints a document', async ({ page }) => {
  const r = await page.evaluate(() => window.paintWebGl());
  expect(r.ok).toBe(true);
  expect(r.colours).toBeGreaterThan(2);
});

test('WebGPU paints a document', async ({ page }) => {
  // Chromium's software WebGPU on Linux CI paints blank ("A valid external Instance reference no longer exists"); being
  // investigated. The path is tested in Edge.
  test.skip(process.env.TWIP_SKIP_WEBGPU === '1', 'WebGPU is skipped on this runner (TWIP_SKIP_WEBGPU)');
  const r = await page.evaluate(() => window.paintWebGpu());
  test.skip(Boolean(r.skipped), r.skipped);
  expect(r.ok, r.error).toBe(true);
  expect(r.colours).toBeGreaterThan(2);
});
