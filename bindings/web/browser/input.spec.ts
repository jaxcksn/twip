// Input in a mounted editor, from real browser events only: typing, selecting, IME, the clipboard, dragging,
// scrolling, zoom, resizing and pixel density. Each is checked through the state and the exported text.
import { expect, test, type Page } from '@playwright/test';
import { docxXml, fixture, hello, mount, open, text } from './helpers.ts';

/** The system clipboard's text, once the editor's write landed (it is asynchronous). */
const clipboard = (page: Page) => expect.poll(() => page.evaluate(() => navigator.clipboard.readText()));
const state = (page: Page, name = 'editor') => page.evaluate((name) => (window as any)[name].getState(), name);
/** The caret in client px: the input element follows it. */
const caret = async (page: Page, name = 'editor') => (await page.locator(`#${name} textarea`).boundingBox())!;
/** Presses `key` and waits for the caret to get there: the input element follows it a moment later. */
async function caretAfter(page: Page, key: string) {
  const before = await caret(page);
  await page.keyboard.press(key);
  await expect.poll(async () => (await caret(page)).x).not.toBe(before.x);
  return caret(page);
}

/** A blank editor with `words` typed into it, focused. */
async function typed(page: Page, words: string, name = 'editor') {
  await page.locator(`#${name} canvas`).click({ position: { x: 300, y: 120 } });
  await page.keyboard.type(words);
  await expect.poll(() => text(page, name)).toBe(words);
}

test.beforeEach(async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await open(page);
});

test('typing', async ({ page }) => {
  await mount(page);
  await typed(page, 'Typed in the browser');
  await page.keyboard.press('Enter');
  await page.keyboard.type('Second line');
  await page.keyboard.press('Backspace');
  await expect.poll(() => text(page)).toBe('Typed in the browser\nSecond lin');
});

test('selecting with the mouse and the keyboard', async ({ page }) => {
  await mount(page);
  await typed(page, 'alpha beta gamma');
  const start = await caretAfter(page, 'Home');
  // A double-click selects the word, which typing replaces.
  await page.mouse.dblclick(start.x + 8, start.y + start.height / 2);
  await expect.poll(async () => (await state(page)).selection.kind).toBe('text');
  await page.keyboard.type('ALPHA ');
  await expect.poll(() => text(page)).toBe('ALPHA beta gamma');
  // A drag from the start selects; Shift+arrows extend.
  const end = await caretAfter(page, 'End');
  await page.mouse.move(start.x + 1, start.y + start.height / 2);
  await page.mouse.down();
  await page.mouse.move(end.x - 1, end.y + end.height / 2, { steps: 5 });
  await page.mouse.up();
  await expect.poll(async () => (await state(page)).selection.kind).toBe('text');
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Shift+ArrowLeft');
  await page.keyboard.press('Shift+ArrowLeft');
  await page.keyboard.type('MA');
  await expect.poll(() => text(page)).toBe('ALPHA beta gamMA');
});

test('IME composition', async ({ page }) => {
  await mount(page);
  await typed(page, 'a');
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Input.imeSetComposition', { text: 'に', selectionStart: 1, selectionEnd: 1 });
  await cdp.send('Input.imeSetComposition', { text: 'にほん', selectionStart: 3, selectionEnd: 3 });
  await cdp.send('Input.insertText', { text: '日本' });
  await expect.poll(() => text(page)).toBe('a日本');
});

test('copy, cut and paste', async ({ page }) => {
  await mount(page);
  await typed(page, 'one two');
  await page.keyboard.press('Control+Shift+ArrowLeft');
  await page.keyboard.press('Control+C');
  await clipboard(page).toBe('two');
  await page.keyboard.press('End');
  await page.keyboard.press('Control+V');
  await expect.poll(() => text(page)).toMatch(/^one two ?two$/);
  await page.evaluate(() => navigator.clipboard.writeText(''));
  await page.keyboard.press('Control+Shift+ArrowLeft');
  await page.keyboard.press('Control+X');
  await expect.poll(() => text(page)).toMatch(/^one two ?$/);
  await clipboard(page).toBe('two');
  await page.keyboard.press('Home');
  await page.keyboard.press('Control+V');
  await expect.poll(() => text(page)).toMatch(/^two ?one two ?$/);
});

test("without ClipboardItem, a copy is the selection's text from the state", async ({ page }) => {
  await page.evaluate(() => delete (globalThis as { ClipboardItem?: unknown }).ClipboardItem);
  await mount(page);
  await typed(page, 'one two');
  await page.evaluate(() => navigator.clipboard.writeText(''));
  await page.keyboard.press('Control+Shift+ArrowLeft');
  await expect.poll(async () => (await state(page)).selection.text).toBe('two');
  await page.keyboard.press('Control+C');
  await clipboard(page).toBe('two');
});

test("the context menu's Paste reads the system clipboard", async ({ page }) => {
  await mount(page);
  await typed(page, 'x');
  await page.evaluate(() => navigator.clipboard.writeText('menu'));
  await page.evaluate(async () => {
    const editor = (window as any).editor;
    const menu = await editor.menus.contextMenu();
    const paste = menu.items.find((i: { command?: string }) => i.command === 'paste');
    if (!paste?.enabled) throw new Error('Paste is disabled');
    await editor.menus.invoke(paste.action);
  });
  await expect.poll(() => text(page)).toMatch(/^x ?menu$/);
});

test('after a font arrives, the input element is at the caret and a click places it', async ({ page }) => {
  await mount(page, { docx: hello });
  // Any new font lays the document out again, with no edit to ask for it.
  await page.evaluate((bytes) => (window as any).twip.FontLibrary.default().add(new Uint8Array(bytes)), fixture('editor/fonts/zzyzx-unknown.ttf'));
  await page.evaluate(() => (window as any).editor.focus());
  const box = (await page.locator('#editor canvas').boundingBox())!;
  await expect.poll(async () => (await caret(page)).x - box.x).toBeGreaterThan(20);
  const start = await caret(page);
  expect(start.y - box.y).toBeGreaterThan(20);
  // A click a few characters into the first line puts the caret there: typing lands after the document's start.
  await page.mouse.click(start.x + 30, start.y + start.height / 2);
  await expect.poll(async () => (await caret(page)).x).toBeGreaterThan(start.x + 10);
  await page.keyboard.type('|');
  await expect.poll(() => text(page)).toMatch(/^[^|]+\|/);
});

test('dragging text', async ({ page }) => {
  await mount(page);
  await typed(page, 'first second');
  const start = await caretAfter(page, 'Home');
  const end = await caretAfter(page, 'End');
  const y = start.y + start.height / 2;
  await page.mouse.dblclick(start.x + 8, y);
  await expect.poll(async () => (await state(page)).selection.kind).toBe('text');
  // Long enough after the double-click that the press isn't a third click.
  await page.waitForTimeout(600);
  await page.mouse.move(start.x + 8, y);
  await page.mouse.down();
  await page.mouse.move(end.x + 2, y, { steps: 10 });
  await page.mouse.up();
  await expect.poll(() => text(page)).toMatch(/^ ?second ?first ?$/);
});

test('scrolling, Ctrl+wheel zoom and resizing the container', async ({ page }) => {
  await mount(page, { docx: fixture('editor/FX-PAGES.docx') });
  const scroller = page.locator('#editor [data-twip="editor"]');
  const pages = () => page.evaluate(() => (window as any).editor.view.visiblePages().map((p: { page: number }) => p.page));
  await expect.poll(pages).toContain(0);
  await scroller.evaluate((e) => (e.scrollTop = 1500));
  await expect.poll(pages).not.toContain(0);
  await page.mouse.move(300, 200);
  await page.keyboard.down('Control');
  await page.mouse.wheel(0, -100);
  await page.keyboard.up('Control');
  await expect.poll(async () => (await state(page)).view.zoom).toBeGreaterThan(100);
  await page.evaluate(() => (document.getElementById('editor')!.style.width = '400px'));
  await expect.poll(() => page.locator('#editor canvas').evaluate((c) => c.getBoundingClientRect().width)).toBeLessThanOrEqual(400);
});

test('a devicePixelRatio change draws at the new density', async ({ page }) => {
  await mount(page);
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1000, height: 800, deviceScaleFactor: 2, mobile: false });
  await expect
    .poll(() => page.locator('#editor canvas').evaluate((c: HTMLCanvasElement) => c.width / c.getBoundingClientRect().width))
    .toBeCloseTo(2, 1);
});

test('two editors on one page: a copy in one pastes into the other', async ({ page }) => {
  await mount(page, { name: 'a', height: 240 });
  await mount(page, { name: 'b', height: 240 });
  await typed(page, 'shared', 'a');
  await page.keyboard.press('Control+A');
  await page.keyboard.press('Control+C');
  await clipboard(page).toBe('shared');
  await page.locator('#b canvas').click({ position: { x: 300, y: 120 } });
  await page.keyboard.press('Control+V');
  await expect.poll(() => text(page, 'b')).toBe('shared');
});

test('two editors on one page: bold text copied from one stays bold in the other', async ({ page }) => {
  await mount(page, { name: 'a', height: 240 });
  await mount(page, { name: 'b', height: 240 });
  await typed(page, 'bold', 'a');
  await page.keyboard.press('Control+A');
  await page.keyboard.press('Control+B');
  await page.keyboard.press('Control+C');
  await page.locator('#b canvas').click({ position: { x: 300, y: 120 } });
  await page.keyboard.press('Control+V');
  await page.keyboard.press('Control+A');
  await expect.poll(async () => (await state(page, 'b')).formatting.bold).toBe(true);
});

test.describe('touch', () => {
  test.use({ hasTouch: true });

  test('a tap places the caret, which typing then uses', async ({ page }) => {
    await mount(page);
    await typed(page, 'tapped');
    const start = await caretAfter(page, 'Home');
    await page.keyboard.press('End');
    await page.touchscreen.tap(start.x + 1, start.y + start.height / 2);
    await page.keyboard.type('>');
    await expect.poll(() => text(page)).toBe('>tapped');
  });
});

test('host keys emit hostCommand, and the browser’s Save, Print and Find stay shut', async ({ page }) => {
  await mount(page);
  await typed(page, 'keys');
  await page.evaluate(() => {
    const w = window as any;
    w.commands = [];
    w.defaults = [];
    w.editor.on('hostCommand', (e: { command: string }) => w.commands.push(e.command));
    addEventListener('keydown', (e) => w.defaults.push(e.defaultPrevented));
  });
  for (const key of ['Control+S', 'Control+P', 'Control+F']) await page.keyboard.press(key);
  await expect.poll(() => page.evaluate(() => (window as any).commands)).toEqual(['save', 'print', 'find']);
  // Each chord's keydown was prevented; the bare Control presses weren't.
  const defaults: boolean[] = await page.evaluate(() => (window as any).defaults);
  expect(defaults.filter(Boolean)).toHaveLength(3);
});

test('Ctrl+D asks for the font dialog, and the browser’s bookmark dialog stays shut', async ({ page }) => {
  await mount(page);
  await typed(page, 'keys');
  await page.evaluate(() => {
    const w = window as any;
    w.dialogs = [];
    w.defaults = [];
    w.editor.on('dialogRequested', (e: { kind: string }) => w.dialogs.push(e.kind));
    addEventListener('keydown', (e) => e.code === 'KeyD' && w.defaults.push(e.defaultPrevented));
  });
  await page.keyboard.press('Control+D');
  await expect.poll(() => page.evaluate(() => (window as any).dialogs)).toEqual(['font']);
  expect(await page.evaluate(() => (window as any).defaults)).toEqual([true]);
});

test('text and images dropped from outside the editor paste at the drop point', async ({ page }) => {
  await mount(page);
  await typed(page, 'drop here');
  const end = await caret(page);
  await page.evaluate(async ({ x, y }) => {
    const target = document.querySelector('#editor canvas')!;
    const drag = (type: string, data: DataTransfer) => target.dispatchEvent(new DragEvent(type, { dataTransfer: data, clientX: x, clientY: y, bubbles: true, cancelable: true }));
    const text = new DataTransfer();
    text.setData('text/plain', ' text');
    drag('dragover', text);
    drag('drop', text);
    // A 1 x 1 PNG.
    const png = Uint8Array.from(atob('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg=='), (c) => c.charCodeAt(0));
    const image = new DataTransfer();
    image.items.add(new File([png], 'dot.png', { type: 'image/png' }));
    drag('dragover', image);
    drag('drop', image);
  }, { x: end.x + 1, y: end.y + end.height / 2 });
  await expect.poll(() => text(page)).toBe('drop here text');
  const docx = await page.evaluate(async () => [...(await (window as any).editor.export({ format: 'docx' })).bytes]);
  expect(docxXml(Buffer.from(docx))).toContain('<w:drawing>');
});

test('a paste from the application reads the system clipboard', async ({ page }) => {
  await mount(page);
  await typed(page, 'x');
  await page.evaluate(() => navigator.clipboard.writeText('pasted'));
  await page.evaluate(() => (window as any).editor.commands.paste());
  await expect.poll(() => text(page)).toMatch(/^x ?pasted$/);
});

test.describe('without clipboard permission', () => {
  test('a paste from the application rejects not-allowed', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    await open(page);
    await mount(page);
    const code = await page.evaluate(() => (window as any).editor.commands.paste().then(() => 'pasted', (e: { code: string }) => e.code));
    expect(code).toBe('not-allowed');
    await context.close();
  });
});
