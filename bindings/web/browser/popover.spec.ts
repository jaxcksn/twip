// The mounted editor's popovers (the tab type picker, a tab stop's options, the AutoCorrect menu) and its theme under
// the platform's forced colours.
import { expect, test, type Page } from '@playwright/test';
import { fixture, mount, open, text } from './helpers.ts';

/** How deep the rulers' bands are, CSS px. */
const BAND = 20;

const state = (page: Page) => page.evaluate(() => (window as any).editor.getState());
const canvas = (page: Page) => page.locator('#editor canvas');
const popover = (page: Page) => page.locator('[data-twip="popover"]');
const run = (page: Page, command: string, arg?: unknown) => page.evaluate(([c, a]) => (window as any).editor.commands[c as string](a), [command, arg]);

test.describe('popovers', () => {
  test.beforeEach(async ({ page }) => {
    await open(page);
    await mount(page, { width: 900, height: 600, docx: fixture('editor/FX-tabs.docx'), mount: { rulers: true } });
    await canvas(page).click({ position: { x: 400, y: 200 } });
    await page.evaluate(() => (window as any).editor.elements.list());
  });

  test('the tab type picker opens on the corner box, answers from the keyboard and gives the focus back', async ({ page }) => {
    const box = (await canvas(page).boundingBox())!;
    await page.mouse.click(box.x + BAND / 2, box.y + BAND / 2);
    await expect(popover(page)).toBeVisible();
    await expect(popover(page)).toContainText('TAB TYPE FOR NEW STOPS');
    await expect(popover(page)).toContainText('Then click the ruler to place it.');
    expect((await popover(page).boundingBox())!.width).toBe(260);
    const items = page.getByRole('menuitemradio');
    await expect(items).toHaveCount(7);
    await expect(items.first()).toHaveAttribute('aria-checked', 'true');
    await expect(items.first()).toBeFocused();
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await expect(popover(page)).toBeHidden();
    await expect.poll(async () => (await state(page)).view.tabSelector).toBe('decimal');
    await expect(page.locator('#editor textarea')).toBeFocused();
    // From the keyboard, and Esc closes it.
    await run(page, 'showTabTypes');
    await expect(popover(page)).toBeVisible();
    await expect(page.getByRole('menuitemradio', { name: /^Decimal/ })).toHaveAttribute('aria-checked', 'true');
    await page.keyboard.press('Escape');
    await expect(popover(page)).toBeHidden();
    await expect(page.locator('#editor textarea')).toBeFocused();
  });

  test("a tab stop's popover changes its alignment and leader, staying open, and clears it", async ({ page }) => {
    await page.evaluate(() => (window as any).editor.find.next({ text: 'Name' }));
    await run(page, 'clearTabStops');
    await run(page, 'addTabStop', { position: 144, alignment: 'left' });
    await run(page, 'showTabStop');
    await expect(popover(page)).toBeVisible();
    await expect(popover(page)).toContainText('Left tab · 2″');
    expect((await popover(page).boundingBox())!.width).toBe(240);
    const stop = async () => (await state(page)).ruler.horizontal.tabs.find((t: { position: number }) => Math.abs(t.position - 144) < 0.01);
    await page.getByRole('menuitemradio', { name: 'Right' }).click();
    await expect.poll(async () => (await stop())?.alignment).toBe('right');
    await page.getByRole('menuitemradio', { name: /^Dots/ }).click();
    await expect.poll(async () => (await stop())?.leader).toBe('dots');
    await expect(popover(page)).toBeVisible();
    await expect(page.getByRole('menuitemradio', { name: 'Right' })).toHaveAttribute('aria-checked', 'true');
    await page.getByRole('menuitem', { name: 'Clear tab stop' }).click();
    await expect.poll(stop).toBeUndefined();
    await expect(popover(page)).toBeHidden();
  });

  test('an application listening for popoverRequested gets it in client px, and the editor shows none', async ({ page }) => {
    await page.evaluate(() => {
      const w = window as any;
      w.asked = [];
      w.editor.on('popoverRequested', (p: unknown) => w.asked.push(p));
    });
    await run(page, 'showTabTypes');
    await expect.poll(() => page.evaluate(() => (window as any).asked.length)).toBe(1);
    const asked = await page.evaluate(() => (window as any).asked[0]);
    const box = (await canvas(page).boundingBox())!;
    expect(asked.kind).toBe('tabType');
    expect(asked.anchor.left).toBeCloseTo(box.x, 0);
    await expect(popover(page)).toBeHidden();
  });

  test('the AutoCorrect menu reverts the correction', async ({ page }) => {
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Enter');
    await page.keyboard.type('teh ');
    await expect.poll(async () => (await state(page)).commands.revertAutoCorrect.enabled).toBe(true);
    await run(page, 'showAutoCorrectOptions');
    await expect(popover(page)).toBeVisible();
    await expect(popover(page)).toContainText('autocorrect');
    await page.getByRole('menuitem', { name: /^Revert to teh/ }).click();
    await expect(popover(page)).toBeHidden();
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
