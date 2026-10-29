// The caret the engine draws: shown while the editor has the focus, blinking, solid again on input, steady under
// reduced motion, and gone without the focus. Checked from screenshots of the canvas, in main-thread and worker mode.
import { expect, test, type Page } from '@playwright/test';
import { mount, open, pixels } from './helpers.ts';

/** The caret in client px: the input element follows it. */
const caret = async (page: Page) => (await page.locator('#editor textarea').boundingBox())!;

/** Dark pixels in a box around the caret: more while the caret's bar is drawn (the text beside it is there either way). */
async function ink(page: Page): Promise<number> {
  const canvas = page.locator('#editor canvas');
  const [c, box, p] = [await caret(page), (await canvas.boundingBox())!, await pixels(page, canvas)];
  const dpr = p.width / box.width;
  let n = 0;
  for (let y = c.y - box.y + 2; y < c.y - box.y + c.height - 2; y++) {
    for (let x = c.x - box.x - 2; x <= c.x - box.x + 3; x++) {
      if (((p.at(x * dpr, y * dpr) >> 16) & 0xff) < 100) n++;
    }
  }
  return n;
}

/** The distinct amounts of ink seen in the caret box over `ms`, fewest first: one while steady, two while blinking. */
async function levels(page: Page, ms: number): Promise<number[]> {
  const seen = new Set<number>();
  for (const end = Date.now() + ms; Date.now() < end; ) seen.add(await ink(page));
  return [...seen].sort((a, b) => a - b);
}

for (const execution of ['main', 'worker'] as const) {
  test.describe(execution, () => {
    test.beforeEach(async ({ page }) => open(page));

    test('the caret shows while focused and blinks', async ({ page }) => {
      await mount(page, { mount: { execution, caretBlink: 200 } });
      expect(await ink(page), 'no focus: no caret').toBe(0);
      await page.locator('#editor canvas').click({ position: { x: 300, y: 120 } });
      await page.keyboard.type('a');
      await expect.poll(() => ink(page), { message: 'typing shows the caret' }).toBeGreaterThan(0);
      const seen = await levels(page, 1500);
      expect(seen, 'shown and hidden phases').toHaveLength(2);
      // Leaving the editor hides it.
      await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
      await expect.poll(() => ink(page), { message: 'blurred: no caret' }).toBe(seen[0]);
    });
  });
}

test.describe('reduced motion', () => {
  test.use({ reducedMotion: 'reduce' });
  test.beforeEach(async ({ page }) => open(page));

  test('the caret is steady', async ({ page }) => {
    await mount(page, { mount: { execution: 'main' } });
    await page.locator('#editor canvas').click({ position: { x: 300, y: 120 } });
    await page.keyboard.type('a');
    await expect.poll(() => ink(page)).toBeGreaterThan(0);
    const seen = await levels(page, 1500);
    expect(seen, 'never hidden').toHaveLength(1);
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    await expect.poll(() => ink(page), { message: 'the steady caret was drawn' }).toBeLessThan(seen[0]!);
  });
});
