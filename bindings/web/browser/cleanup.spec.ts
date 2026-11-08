// Repeated mounts and destroys, and failed mounts, leave nothing behind: no SDK DOM, listeners, observers, timers,
// frames or workers (the host's resource ledger), and the heap returns to where it started.
import { expect, test } from '@playwright/test';
import { hello, open, resources } from './helpers.ts';

test.use({ launchOptions: { args: ['--js-flags=--expose-gc', '--enable-unsafe-webgpu'] } });

test('100 mount and destroy cycles and 20 failed mounts leave nothing', async ({ page }) => {
  test.setTimeout(180_000);
  await open(page);
  const heap = () =>
    page.evaluate(async () => {
      for (let i = 0; i < 3; i++) {
        (globalThis as any).gc();
        await new Promise((r) => setTimeout(r, 20));
      }
      return (performance as any).memory.usedJSHeapSize as number;
    });
  const cycle = (docx: number[], count: number, failing: boolean) =>
    page.evaluate(
      async ({ docx, count, failing }) => {
        const { Editor } = (window as any).twip;
        const container = document.createElement('div');
        container.style.cssText = 'width:640px;height:480px;';
        document.body.append(container);
        let failed = 0;
        for (let i = 0; i < count; i++) {
          const bytes = failing ? new Uint8Array([80, 75, 3, 4, i]) : new Uint8Array(docx);
          try {
            const editor = await Editor.mount(container, { document: bytes });
            await editor.commands.insertText('x');
            await editor.destroy();
          } catch {
            failed++;
          }
        }
        container.remove();
        return failed;
      },
      { docx, count, failing },
    );
  // Warm up first: the engine module, its caches and the compiled code are the page's, not a mount's.
  await cycle(hello, 50, false);
  await cycle(hello, 10, true);
  const start = await heap();
  expect(await cycle(hello, 100, false)).toBe(0);
  expect(await cycle(hello, 20, true)).toBe(20);
  // Measured before any locator: Playwright's injected script is in the same heap.
  expect(await heap()).toBeLessThanOrEqual(start * 1.1);
  expect(await page.locator('[data-twip]').count()).toBe(0);
  expect(await resources(page)).toEqual({ nodes: 0, listeners: 0, observers: 0, timers: 0, frames: 0, workers: 0 });
});
