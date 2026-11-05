// The placeholder affordances on a mounted editor: `placeholderSelected` in client px, sent again as its anchor moves,
// Tab between placeholders, and Escape then Tab still leaving the editor.
import { expect, test, type Page } from '@playwright/test';
import { fixture, mount, open } from './helpers.ts';

const input = '#editor [data-twip="input"]';

/** The `placeholderSelected` events so far. */
const selected = (page: Page) => page.evaluate(() => (window as any).selected as ({ key?: string; element?: string; anchor: { left: number; top: number } } | null)[]);
const last = async (page: Page) => (await selected(page)).at(-1);

test('placeholderSelected follows the selection and the scroll, and Tab moves between placeholders', async ({ page }) => {
  await open(page);
  await mount(page, { width: 900, height: 600, docx: fixture('editor/FX-PLACEHOLDER-UI.docx') });
  await page.evaluate(() => document.querySelector('#editor')!.after(Object.assign(document.createElement('button'), { id: 'after', textContent: 'After' })));
  await page.evaluate(async () => {
    const w = window as any;
    w.selected = [];
    w.editor.on('placeholderSelected', (t: unknown) => w.selected.push(t));
    await w.editor.commands.setMode('design');
    await w.editor.view.setPlaceholderUi(true);
  });
  expect((await page.evaluate(() => (window as any).editor.getState())).view.placeholderUi).toBe(true);

  // A field selected from the application: its anchor in client px, on the canvas.
  await page.locator(input).focus();
  await page.evaluate(async () => {
    const e = (window as any).editor;
    const fields = await e.elements.list({ kind: 'field' });
    await e.elements.select(fields.find((f: { code: string }) => f.code.includes('Applicant.First_Name')).id);
  });
  await expect.poll(async () => (await last(page))?.key).toBe('Applicant.First_Name');
  const box = (await page.locator('#editor canvas').boundingBox())!;
  const first = (await last(page))!;
  expect(first.anchor.left).toBeGreaterThanOrEqual(box.x);
  expect(first.anchor.top).toBeGreaterThanOrEqual(box.y);

  // A scroll moves the anchor, and the event comes again.
  const count = (await selected(page)).length;
  await page.locator('#editor [data-twip="editor"]').evaluate((e) => (e.scrollTop = 40));
  await expect.poll(async () => (await selected(page)).length).toBeGreaterThan(count);
  const scrolled = (await last(page))!;
  expect(scrolled.key).toBe('Applicant.First_Name');
  expect(scrolled.anchor.top).toBeLessThan(first.anchor.top);

  // Tab selects the next placeholder.
  await page.keyboard.press('Tab');
  await expect.poll(async () => (await last(page))?.element ?? first.element).not.toBe(first.element);
  await expect(page.locator(input)).toBeFocused();

  // Escape then Tab still leaves the editor.
  await page.keyboard.press('Escape');
  await page.keyboard.press('Tab');
  await expect(page.locator('#after')).toBeFocused();
});
