// preload(): the engine worker starts with no editor, the first editor uses it, and it goes with that editor.
import { expect, test } from '@playwright/test';
import { hello, open, resources } from './helpers.ts';

test('a preloaded worker serves the first editor and leaves with it', async ({ page }) => {
  await open(page);
  const engine: string[] = [];
  page.on('request', (r) => {
    if (r.url().includes('twip_wasm.wasm')) engine.push(r.url());
  });
  await page.evaluate(() => (window as any).twip.preload({ execution: 'worker' }));
  expect((await resources(page)).workers).toBe(1);
  const fetched = engine.length;
  const workers = await page.evaluate(async (docx) => {
    const { Editor } = (window as any).twip;
    const container = document.createElement('div');
    container.style.cssText = 'width:640px;height:480px;';
    document.body.append(container);
    const editor = await Editor.mount(container, { document: new Uint8Array(docx), execution: 'worker' });
    const during = (globalThis as any)[Symbol.for('twip.resources')]().workers;
    await editor.destroy();
    container.remove();
    return during;
  }, hello);
  expect(engine.length, 'the editor fetched no engine of its own').toBe(fetched);
  expect(workers, 'one worker: the preloaded one').toBe(1);
  await expect.poll(async () => (await resources(page)).workers).toBe(0);
});

test('preload calls share one load', async ({ page }) => {
  await open(page);
  const same = await page.evaluate(() => {
    const { preload } = (window as any).twip;
    return preload({ execution: 'worker' }) === preload({ execution: 'worker' });
  });
  expect(same).toBe(true);
});
