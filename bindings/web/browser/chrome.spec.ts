// What an application's chrome is built on, in a real browser: the context menu, the markup pane and balloons, the
// background, tooltips and drops from outside the editor.
import { expect, test, type Page } from '@playwright/test';
import { mount, open, pixels, text } from './helpers.ts';

const editor = (page: Page) => page.locator('#editor canvas');
/** The caret in client px: the input element follows it. */
const caret = async (page: Page) => (await page.locator('#editor textarea').boundingBox())!;

/** Records the browser's own handling: whether the last `contextmenu` and `keydown` reached it unprevented. */
async function watchDefaults(page: Page) {
  await page.evaluate(() => {
    const w = window as any;
    w.defaults = { contextmenu: [] as boolean[], keydown: [] as boolean[] };
    addEventListener('contextmenu', (e) => w.defaults.contextmenu.push(!e.defaultPrevented));
    addEventListener('keydown', (e) => w.defaults.keydown.push(!e.defaultPrevented));
  });
}

test.beforeEach(({ page }) => open(page));

test('with a contextMenu listener the browser menu stays shut and the model is emitted', async ({ page }) => {
  await mount(page);
  await watchDefaults(page);
  await editor(page).click({ position: { x: 300, y: 120 } });
  await page.keyboard.type('menu');
  await page.evaluate(() => {
    const w = window as any;
    w.menus = [];
    w.off = w.editor.on('contextMenu', (e: { via: string; at: object; menu: { items: unknown[] } }) => w.menus.push(e));
  });
  await editor(page).click({ button: 'right', position: { x: 320, y: 130 } });
  await page.keyboard.press('ContextMenu');
  await page.keyboard.press('Shift+F10');
  await expect.poll(() => page.evaluate(() => (window as any).menus.map((m: { via: string }) => m.via))).toEqual(['pointer', 'keyboard', 'keyboard']);
  const first = await page.evaluate(() => (window as any).menus[0]);
  expect(first.menu.items.length).toBeGreaterThan(0);
  // The pointer's menu opens where the pointer is, in client px.
  const box = (await editor(page).boundingBox())!;
  expect(first.at.clientX).toBeCloseTo(box.x + 320, 0);
  expect(first.at.clientY).toBeCloseTo(box.y + 130, 0);
  expect(await page.evaluate(() => (window as any).defaults.contextmenu)).not.toContain(true);
  // Without a listener the browser's menu opens.
  await page.evaluate(() => (window as any).off());
  await page.evaluate(() => ((window as any).defaults.contextmenu = []));
  await editor(page).click({ button: 'right', position: { x: 320, y: 130 } });
  await expect.poll(() => page.evaluate(() => (window as any).defaults.contextmenu)).toEqual([true]);
  await page.keyboard.press('Escape');
});

test('a markup pane moves the pages aside, and balloons stay level with their anchors', async ({ page }) => {
  await mount(page, { width: 900, height: 500 });
  await editor(page).click({ position: { x: 300, y: 120 } });
  for (let i = 1; i <= 3; i++) {
    await page.keyboard.type(`Paragraph ${i} with a comment.`);
    await page.keyboard.press('Enter');
  }
  const left = () => page.evaluate(() => (window as any).editor.view.visiblePages()[0]?.rect.left as number);
  await expect.poll(left).toBeGreaterThan(0);
  const before = await left();
  await page.evaluate(() => (window as any).editor.review.setMarkupPaneWidth(240));
  await expect.poll(left).toBeLessThan(before);

  // Comments on the three paragraphs: Ctrl+Home, then select a word with Shift+Ctrl+Right, comment, move down.
  await editor(page).click({ position: { x: 300, y: 120 } });
  await page.keyboard.press('Control+Home');
  const threads: string[] = [];
  for (let i = 0; i < 3; i++) {
    await page.keyboard.press('Control+Shift+ArrowRight');
    threads.push(await page.evaluate((i) => (window as any).editor.commands.addComment(`Comment ${i}`).then((r: { id: string }) => r.id), i));
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Home');
  }
  await page.evaluate((threads) => {
    const w = window as any;
    w.laidOut = [];
    w.editor.on('balloonsLaidOut', (p: unknown[]) => w.laidOut.push(p));
    for (const thread of threads) {
      const card = document.createElement('div');
      card.className = 'card';
      card.dataset.thread = thread;
      card.style.cssText = 'width:200px;height:16px;background:#fff;border:1px solid #888;';
      w.editor.review.attachBalloon(thread, card);
    }
  }, threads);
  const level = async () => {
    const r = await page.evaluate(() => {
      const editor = (window as any).editor;
      const layer = editor.review.balloonLayer.getBoundingClientRect();
      const cards = [...editor.review.balloonLayer.querySelectorAll('.card')].map((c: Element) => {
        const b = c.getBoundingClientRect();
        return { thread: (c as HTMLElement).dataset.thread, top: b.top - layer.top, bottom: b.bottom - layer.top, shown: (c as HTMLElement).style.display !== 'none' };
      });
      return { cards, anchors: editor.review.anchors() };
    });
    const shown = r.cards.filter((c) => c.shown).sort((a, b) => a.top - b.top);
    for (let i = 1; i < shown.length; i++) expect(shown[i]!.top).toBeGreaterThanOrEqual(shown[i - 1]!.bottom);
    // Cards shorter than the line spacing sit level with their lines.
    return shown.map((c) => Math.abs(c.top - r.anchors.find((a: { thread: string }) => a.thread === c.thread).lineTop));
  };
  await expect.poll(async () => (await level()).length).toBe(3);
  const within = async () => Math.max(...(await level())) <= 1;
  await expect.poll(within).toBe(true);
  expect(await page.evaluate(() => (window as any).laidOut.length)).toBeGreaterThan(0);
  // Zoom, then an edit above the comments, then a scroll.
  await page.evaluate(() => (window as any).editor.commands.setZoom(150));
  await expect.poll(within).toBe(true);
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('Enter');
  await expect.poll(within).toBe(true);
  await page.locator('#editor [data-twip="editor"]').evaluate((e) => (e.scrollTop = 40));
  await expect.poll(within).toBe(true);

});

test('a dark background colours the area around the pages, and the paper stays white', async ({ page }) => {
  await mount(page);
  await page.evaluate(() => (window as any).editor.view.setBackground('#1E1E1E'));
  await expect.poll(async () => (await pixels(page, editor(page))).at(2, 2)).toBe(0x1e1e1e);
  const [p] = await page.evaluate(() => (window as any).editor.view.visiblePages());
  const box = (await editor(page).boundingBox())!;
  const shot = await pixels(page, editor(page));
  const scale = shot.width / box.width;
  // A point in the page's top margin.
  expect(shot.at((p.rect.left - box.x + 20) * scale, (p.rect.top - box.y + 20) * scale)).toBe(0xffffff);
  expect(await page.locator('#editor [data-twip="editor"]').evaluate((e) => getComputedStyle(e).backgroundColor)).toBe('rgb(30, 30, 30)');
});

test('hovering a link emits a tooltip', async ({ page }) => {
  await mount(page);
  await editor(page).click({ position: { x: 300, y: 120 } });
  await page.evaluate(() => (window as any).editor.commands.insertHyperlink({ address: 'https://example.com/', text: 'a link here' }));
  const end = await caret(page);
  await page.keyboard.press('Home');
  // The input element follows the caret when the engine's request arrives, after the key.
  await expect.poll(async () => (await caret(page)).x).toBeLessThan(end.x);
  const start = await caret(page);
  await page.evaluate(() => {
    const w = window as any;
    w.tooltips = [];
    w.editor.on('tooltip', (t: unknown) => w.tooltips.push(t));
  });
  await page.mouse.move(start.x + 20, start.y + start.height / 2, { steps: 3 });
  await expect.poll(() => page.evaluate(() => (window as any).tooltips.filter(Boolean).length)).toBeGreaterThan(0);
  const tip = await page.evaluate(() => (window as any).tooltips.find(Boolean));
  expect(tip.text).toContain('example.com');
  expect(tip.at.clientX).toBeGreaterThan(0);
});

test('a field dragged from a list outside the editor shows the drop caret and inserts on drop', async ({ page }) => {
  // A steady caret: only the drop caret changes the pixels.
  await mount(page, { mount: { caretBlink: 0 } });
  await editor(page).click({ position: { x: 300, y: 120 } });
  await page.keyboard.type('Dear ');
  // The input element is at the caret once the typing is in.
  await expect.poll(() => text(page)).toContain('Dear');
  await page.evaluate(() => {
    const w = window as any;
    const item = Object.assign(document.createElement('div'), { id: 'field', textContent: 'FirstName', draggable: true });
    item.style.cssText = 'width:120px;padding:4px;background:#ddd;';
    item.addEventListener('dragstart', (e) => e.dataTransfer!.setData('application/x-field', 'FirstName'));
    document.body.prepend(item);
    const container = w.editor.container as HTMLElement;
    container.addEventListener('dragover', (e) => {
      if (!e.dataTransfer!.types.includes('application/x-field')) return;
      e.preventDefault();
      w.editor.elements.dragOver({ clientX: e.clientX, clientY: e.clientY });
    });
    container.addEventListener('dragleave', () => w.editor.elements.dragLeave());
    container.addEventListener('drop', (e) => {
      e.preventDefault();
      w.dropped = w.editor.elements.drop({ clientX: e.clientX, clientY: e.clientY }, { type: 'insertField', code: 'MERGEFIELD FirstName', display: '«FirstName»' });
    });
  });
  const end = await caret(page);
  const before = await pixels(page, editor(page));
  await page.locator('#field').hover();
  await page.mouse.down();
  await page.mouse.move(end.x + 30, end.y + end.height / 2, { steps: 5 });
  await page.mouse.move(end.x + 2, end.y + end.height / 2, { steps: 5 });
  // The drop caret: the editor's pixels changed with nothing edited.
  const during = await pixels(page, editor(page));
  let changed = 0;
  for (let y = 0; y < during.height; y += 1) for (let x = 0; x < during.width; x += 4) if (during.at(x, y) !== before.at(x, y)) changed++;
  expect(changed).toBeGreaterThan(0);
  await page.mouse.up();
  await page.evaluate(() => (window as any).dropped);
  await expect.poll(() => text(page)).toContain('Dear');
  expect(await page.evaluate(async () => (await (window as any).editor.elements.list()).map((e: { code?: string }) => e.code?.trim()))).toEqual(['MERGEFIELD FirstName']);
});
