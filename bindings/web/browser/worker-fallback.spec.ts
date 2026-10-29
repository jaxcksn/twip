// Where the engine runs: `execution: 'auto'` (the default) is the worker where the browser can move a canvas to one,
// and the main thread where it can't. Either way the API is the same: `getState()` stays synchronous, and the keys the
// editor claims are claimed in `keydown` itself, before the worker could answer.
import { expect, test } from '@playwright/test';
import { mount, open, resources, text } from './helpers.ts';

const mode = (page: import('@playwright/test').Page) => page.evaluate(() => (window as any).editor.executionMode);

test('without transferControlToOffscreen, auto runs on the main thread', async ({ page }) => {
  await open(page);
  await page.evaluate(() => delete (HTMLCanvasElement.prototype as Partial<HTMLCanvasElement>).transferControlToOffscreen);
  await mount(page);
  expect(await mode(page)).toBe('main');
  await page.evaluate(() => (window as any).editor.commands.insertText('On the main thread'));
  expect(await text(page)).toBe('On the main thread');
  expect(await resources(page)).toMatchObject({ workers: 0 });
  // Forcing the worker there fails, and leaves nothing behind.
  const forced = await page.evaluate(async () => {
    const container = document.createElement('div');
    document.body.append(container);
    return (window as any).twip.Editor.mount(container, { execution: 'worker' }).then(() => 'mounted', (e: any) => e.code);
  });
  expect(forced).toBe('unsupported');
});

test('auto runs in a worker where it can, and getState() stays synchronous', async ({ page }) => {
  await open(page);
  await mount(page);
  expect(await mode(page)).toBe('worker');
  expect(await resources(page)).toMatchObject({ workers: 1 });
  const states = await page.evaluate(async () => {
    const editor = (window as any).editor;
    const before = editor.getState();
    await editor.commands.insertText('In the worker');
    const after = editor.getState();
    return { sync: !(before instanceof Promise), before: before.revision, after: after.revision, modified: after.modified };
  });
  expect(states).toEqual({ sync: true, before: states.before, after: states.before + 1, modified: true });
  expect(await text(page)).toBe('In the worker');
  // A forced main thread beside it.
  await mount(page, { name: 'main', mount: { execution: 'main' } });
  expect(await page.evaluate(() => (window as any).main.executionMode)).toBe('main');
  await page.evaluate(() => Promise.all([(window as any).editor.destroy(), (window as any).main.destroy()]));
  await expect.poll(() => resources(page)).toMatchObject({ workers: 0 });
});

test('a claimed key is claimed in keydown, in the worker', async ({ page }) => {
  await open(page);
  await mount(page);
  const prevented = await page.evaluate(() => {
    const input = document.querySelector('#editor textarea')!;
    const press = (init: KeyboardEventInit) => {
      const e = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });
      input.dispatchEvent(e);
      return e.defaultPrevented;
    };
    // Ctrl+B is bold's, Ctrl+S the host's save; Ctrl+J here is nobody's.
    return {
      bold: press({ code: 'KeyB', key: 'b', ctrlKey: true }),
      save: press({ code: 'KeyS', key: 's', ctrlKey: true }),
      free: press({ code: 'KeyJ', key: 'j', ctrlKey: true, altKey: true }),
    };
  });
  expect(prevented).toEqual({ bold: true, save: true, free: false });
});
