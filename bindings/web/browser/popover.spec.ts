// The popover requests (the tab type picker, a tab stop's options, the AutoCorrect menu): the application draws
// them, the editor draws none. And the theme under the platform's forced colours.
import { expect, test, type Page } from '@playwright/test';
import { fixture, mount, open, text } from './helpers.ts';

/** How deep the rulers' bands are, CSS px. */
const BAND = 22;

const state = (page: Page) => page.evaluate(() => (window as any).editor.getState());
const canvas = (page: Page) => page.locator('#editor canvas');
const run = (page: Page, command: string, arg?: unknown) => page.evaluate(([c, a]) => (window as any).editor.commands[c as string](a), [command, arg]);
/** The popover events so far, as `kind` for a request and `closed`. */
const asked = (page: Page) => page.evaluate(() => (window as any).asked as string[]);

test.describe('popovers', () => {
  test.beforeEach(async ({ page }) => {
    await open(page);
    await mount(page, { width: 900, height: 600, docx: fixture('editor/FX-tabs.docx'), mount: { rulers: true } });
    await canvas(page).click({ position: { x: 400, y: 200 } });
    await page.evaluate(() => {
      const w = window as any;
      w.asked = [];
      w.editor.on('popoverRequested', (p: { kind: string }) => w.asked.push(p.kind));
      w.editor.on('popoverClosed', () => w.asked.push('closed'));
      return w.editor.elements.list();
    });
  });

  test('the corner box asks for the tab type picker in client px, and the editor draws none', async ({ page }) => {
    const box = (await canvas(page).boundingBox())!;
    const anchor = page.evaluate(() => new Promise<any>((resolve) => (window as any).editor.on('popoverRequested', resolve)));
    await page.mouse.click(box.x + BAND / 2, box.y + BAND / 2);
    const p = await anchor;
    expect(p.kind).toBe('tabType');
    expect(p.anchor.left).toBeCloseTo(box.x + 3, 0);
    expect(p.choices).toHaveLength(7);
    expect(await page.locator('[data-twip="popover"]').count()).toBe(0);
    // The answer closes it.
    await run(page, 'setRulerTabType', 'decimal');
    await expect.poll(() => asked(page)).toEqual(['tabType', 'closed']);
    expect((await state(page)).view.tabSelector).toBe('decimal');
  });

  test("a tab stop's popover stays open while it changes, and closePopover closes it", async ({ page }) => {
    await page.evaluate(() => (window as any).editor.find.next({ text: 'Name' }));
    await run(page, 'clearTabStops');
    await run(page, 'addTabStop', { position: 144, alignment: 'left' });
    await run(page, 'showTabStop');
    await expect.poll(() => asked(page)).toEqual(['tabStop']);
    await run(page, 'addTabStop', { position: 144, alignment: 'right', leader: 'dots' });
    await run(page, 'closePopover');
    await expect.poll(() => asked(page)).toEqual(['tabStop', 'closed']);
    await run(page, 'closePopover');
    expect(await asked(page)).toEqual(['tabStop', 'closed']);
  });

  test("the AutoCorrect menu's answer reverts the correction and closes it", async ({ page }) => {
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Enter');
    await page.keyboard.type('teh ');
    await expect.poll(async () => (await state(page)).commands.revertAutoCorrect.enabled).toBe(true);
    await run(page, 'showAutoCorrectOptions');
    await expect.poll(() => asked(page)).toEqual(['autoCorrect']);
    await run(page, 'revertAutoCorrect');
    await expect.poll(() => asked(page)).toEqual(['autoCorrect', 'closed']);
    await expect.poll(() => text(page)).toContain('teh ');
  });
});

test('forced colours set the high-contrast theme and a light page, and the application\'s once they end', async ({ page }) => {
  await page.emulateMedia({ forcedColors: 'active' });
  await open(page);
  await mount(page);
  await expect.poll(async () => typeof (await state(page)).view.theme).toBe('object');
  const view = (await state(page)).view;
  expect(Object.keys(view.theme.highContrast).sort()).toEqual(['canvas', 'canvasText', 'grayText', 'highlight', 'highlightText']);
  expect(view.appearance).toBe('light');
  // The application's choice waits for the mode to end.
  await page.evaluate(() => (window as any).editor.view.setTheme('dark'));
  await page.evaluate(() => (window as any).editor.view.setAppearance('dark'));
  expect(typeof (await state(page)).view.theme).toBe('object');
  await page.emulateMedia({ forcedColors: 'none' });
  await expect.poll(async () => (await state(page)).view.theme).toBe('dark');
  expect((await state(page)).view.appearance).toBe('dark');
});
