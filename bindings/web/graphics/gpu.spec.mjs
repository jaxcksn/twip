// twip-graphics' cross-backend comparisons on the browser's WebGPU: every scene drawn by the GPU backend and by the
// CPU backend agrees within the tolerance (crates/twip-graphics-web). Build them first:
// `node ../../crates/twip-graphics-web/build.mjs`.
import { expect, test } from '@playwright/test';

test('the GPU backend draws every scene as the CPU backend does, on WebGPU', async ({ page }) => {
  await page.goto('/index.html');
  const report = await (await page.waitForFunction(() => window.report, null, { timeout: 120_000 })).jsonValue();
  expect(report.error, 'a WebGPU adapter').toBeUndefined();
  console.log(`${report.adapter} (${report.backend}); kept pictures prepared ${report.kept[0]}, drawn again ${report.kept[1]}`);
  for (const s of report.scenes) {
    console.log(`${s.name}: ${s.over} pixels over 2 (at most ${s.allowed}), worst ${s.worst}`);
    expect(s.over, `${s.name}: pixels differing by more than 2`).toBeLessThanOrEqual(s.allowed);
  }
  expect(report.kept, 'kept pictures: prepared once, drawn again on whole-pixel moves').toEqual([4, 8]);
});
