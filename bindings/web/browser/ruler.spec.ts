// The built-in rulers, driven through raw pointer input: tab stops, indents and margins, and the dark look.
import { expect, test, type Page } from '@playwright/test';
import { fixture, mount, open, pixels } from './helpers.ts';

/** How deep the rulers' bands are, CSS px: the horizontal one along the top, the vertical one left. */
const BAND = 20;

const state = (page: Page) => page.evaluate(() => (window as any).editor.getState());
const view = (page: Page) => page.evaluate(() => (window as any).editor.ruler.view());
const canvas = (page: Page) => page.locator('#editor canvas');
/** The client point of `points` from the page edge along the middle of the horizontal ruler. */
async function onRuler(page: Page, points: number) {
  const v = await view(page);
  const box = (await canvas(page).boundingBox())!;
  return { x: v.horizontal.origin + points * v.horizontal.scale, y: box.y + BAND / 2 };
}
/** The client point of a tab stop at `points` (tab stops count from the text's start, the left margin). */
const onTab = async (page: Page, points: number) => onRuler(page, (await state(page)).ruler.horizontal.text.start + points);
const tabs = async (page: Page) => (await state(page)).ruler.horizontal.tabs.filter((t: { inherited: boolean }) => !t.inherited);
const undos = async (page: Page) => (await state(page)).history.undoLabel;

test.describe('rulers', () => {
  test.beforeEach(async ({ page }) => {
    await open(page);
    await mount(page, { width: 900, height: 600, docx: fixture('editor/FX-tabs.docx'), mount: { rulers: true } });
    await canvas(page).click({ position: { x: 400, y: 200 } });
    // A call behind the click: the state then is the clicked paragraph's.
    await page.evaluate(() => (window as any).editor.elements.list());
  });

  test('both rulers show, lined up with the page after scrolling and zooming', async ({ page }) => {
    expect((await state(page)).view.rulers).toEqual({ horizontal: true, vertical: true });
    const dpr = await page.evaluate(() => devicePixelRatio);
    const aligned = async () => {
      const v = await view(page);
      const p = (await page.evaluate(() => (window as any).editor.view.visiblePages())).find((p: { page: number }) => p.page + 1 === v.page);
      return Math.abs(v.horizontal.origin - p.rect.left) <= 1 / dpr && Math.abs(v.vertical.origin - p.rect.top) <= 1 / dpr;
    };
    await expect.poll(aligned).toBe(true);
    await page.locator('#editor [data-twip="editor"]').evaluate((e) => (e.scrollTop = 120));
    await expect.poll(aligned).toBe(true);
    await page.evaluate(() => (window as any).editor.commands.setZoom(150));
    await expect.poll(aligned).toBe(true);
  });

  test('a click adds a stop, a drag moves it, a drag off deletes it', async ({ page }) => {
    const before = (await tabs(page)).length;
    const at = await onTab(page, 145);
    await page.mouse.click(at.x, at.y);
    // Snapped to the inch ruler's eighths (9 pt), of the tab selector's kind.
    await expect.poll(async () => (await tabs(page)).length).toBe(before + 1);
    const added = (await tabs(page)).find((t: { position: number }) => Math.abs(t.position - 144) < 0.01);
    expect(added?.alignment).toBe('left');
    // A drag shows a guide line through the page and commits on release, one undo step.
    const to = await onTab(page, 216);
    const shot = await pixels(page, canvas(page));
    // Past the double-click time, so the press starts a drag rather than asking for the Tabs dialog.
    await page.waitForTimeout(500);
    await page.mouse.move(at.x - 1, at.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 5 });
    const box = (await canvas(page).boundingBox())!;
    const scale = shot.width / box.width;
    // The guide is dashed: a few pixels down the line, some differ.
    const x = (to.x - box.x) * scale;
    await expect
      .poll(async () => {
        const guide = await pixels(page, canvas(page));
        return [0, 1, 2, 3, 4, 5].some((d) => guide.at(x, (300 + d) * scale) !== shot.at(x, (300 + d) * scale));
      })
      .toBe(true);
    await page.mouse.up();
    await expect.poll(async () => (await tabs(page)).some((t: { position: number }) => Math.abs(t.position - 216) < 0.01)).toBe(true);
    expect(await undos(page)).toBeTruthy();
    // Off the ruler: deleted.
    await page.mouse.move(to.x, to.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y + 80, { steps: 5 });
    await page.mouse.up();
    await expect.poll(async () => (await tabs(page)).length).toBe(before);
  });

  test('the corner box cycles the tab selector, and a double-click asks for the Tabs dialog', async ({ page }) => {
    const box = (await canvas(page).boundingBox())!;
    await page.mouse.click(box.x + BAND / 2, box.y + BAND / 2);
    const at = await onTab(page, 288);
    await page.mouse.click(at.x, at.y);
    await expect.poll(async () => (await tabs(page)).find((t: { position: number }) => Math.abs(t.position - 288) < 0.01)?.alignment).toBe('center');
    await page.evaluate(() => {
      const w = window as any;
      w.dialogs = [];
      w.editor.on('dialogRequested', (e: { kind: string }) => w.dialogs.push(e.kind));
    });
    await page.mouse.dblclick(at.x, at.y);
    await expect.poll(() => page.evaluate(() => (window as any).dialogs)).toContain('tabs');
  });

  test('dragging the indent markers and the margins changes them, one undo step each', async ({ page }) => {
    const h = async () => (await state(page)).ruler.horizontal;
    const drag = async (points: number, dy: number, by: number) => {
      const at = await onRuler(page, points);
      await page.mouse.move(at.x, at.y + dy);
      await page.mouse.down();
      await page.mouse.move(at.x + by, at.y + dy, { steps: 5 });
      await page.mouse.up();
    };
    const start = await h();
    const left = start.text.start + start.indents.left;
    // The first-line marker hangs from the top of the band, the hanging and left markers stand on its bottom.
    await drag(left + start.indents.firstLine, -BAND / 2 + 1, 18);
    await expect.poll(async () => (await h()).indents.firstLine).not.toBe(start.indents.firstLine);
    await drag(left, BAND / 2 - 3, 18);
    await expect.poll(async () => (await h()).indents.left).not.toBe(start.indents.left);
    await drag(start.text.end - start.indents.right, BAND / 2 - 3, -18);
    await expect.poll(async () => (await h()).indents.right).not.toBe(start.indents.right);
    await drag(start.text.start, 0, 36);
    await expect.poll(async () => (await h()).text.start).not.toBe(start.text.start);
    // Four steps back undo all four.
    for (let i = 0; i < 4; i++) await page.evaluate(() => (window as any).editor.commands.undo());
    expect(await h()).toEqual(start);
  });

  test('with a dark background the rulers are drawn dark', async ({ page }) => {
    await page.evaluate(() => (window as any).editor.view.setBackground('#1E1E1E'));
    const box = (await canvas(page).boundingBox())!;
    const at = await onRuler(page, 400);
    await expect
      .poll(async () => {
        const p = await pixels(page, canvas(page));
        const c = p.at((at.x - box.x) * (p.width / box.width), (BAND / 2) * (p.width / box.width));
        return ((c >> 16) + ((c >> 8) & 0xff) + (c & 0xff)) / 3;
      })
      .toBeLessThan(128);
  });
});
