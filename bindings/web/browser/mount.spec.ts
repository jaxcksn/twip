// Mounting, opening, exporting and destroying an editor in a vanilla page, through the public API alone.
import { expect, test } from '@playwright/test';
import { colours, docxText, hello, mount, open, resources, text } from './helpers.ts';

test.beforeEach(({ page }) => open(page));

test('the mount resolves with the first viewport drawn', async ({ page }) => {
  await mount(page, { docx: hello });
  expect(await colours(page, page.locator('#editor canvas'))).toBeGreaterThan(2);
  expect(await text(page)).toContain('Hello');
});

test('the CPU path draws where there is no WebGPU or WebGL', async ({ page, context }) => {
  const cpu = await context.newPage();
  await open(cpu, { gpu: false });
  await mount(cpu, { docx: hello });
  expect(await colours(cpu, cpu.locator('#editor canvas'))).toBeGreaterThan(2);
  await cpu.locator('#editor canvas').click({ position: { x: 300, y: 200 } });
  await cpu.keyboard.type('Hi');
  await expect.poll(() => text(cpu)).toContain('Hi');
  void page;
});

test('no document opens a blank one', async ({ page }) => {
  await mount(page);
  expect(await page.evaluate(() => (window as any).editor.getState().pages.count)).toBe(1);
  expect(await text(page)).toBe('');
});

test('a hidden or zero-sized container still mounts, and the editor appears once shown', async ({ page }) => {
  await page.evaluate(async (docx) => {
    const { Editor } = (window as any).twip;
    const hidden = Object.assign(document.createElement('div'), { id: 'hidden' });
    hidden.style.cssText = 'display:none;width:640px;height:480px;';
    const empty = Object.assign(document.createElement('div'), { id: 'empty' });
    empty.style.cssText = 'width:0;height:0;';
    document.body.append(hidden, empty);
    Object.assign(window, {
      hidden: await Editor.mount(hidden, { document: new Uint8Array(docx) }),
      empty: await Editor.mount(empty, { document: new Uint8Array(docx) }),
    });
  }, hello);
  await page.evaluate(() => {
    document.getElementById('hidden')!.style.display = 'block';
    Object.assign(document.getElementById('empty')!.style, { width: '640px', height: '480px' });
  });
  // One read of the pixels takes seconds, and the first may come before the first frame (in the worker, a frame
  // lands a message later): room for a second read.
  await expect.poll(() => colours(page, page.locator('#hidden canvas')), { timeout: 20_000 }).toBeGreaterThan(2);
  await expect.poll(() => colours(page, page.locator('#empty canvas')), { timeout: 20_000 }).toBeGreaterThan(2);
});

test('a second mount into the same element rejects, and the first editor keeps working', async ({ page }) => {
  await mount(page, { docx: hello });
  const code = await page.evaluate(() =>
    (window as any).twip.Editor.mount(document.getElementById('editor')).then(
      () => 'mounted',
      (e: { code: string }) => e.code,
    ),
  );
  expect(code).toBe('unavailable');
  expect(await page.locator('#editor [data-twip="editor"]').count()).toBe(1);
  await page.evaluate(() => (window as any).editor.commands.insertText('Still '));
  expect(await text(page)).toContain('Still ');
});

test('a failed mount leaves no DOM and no resources', async ({ page }) => {
  const before = await resources(page);
  const code = await page.evaluate(() => {
    const container = Object.assign(document.createElement('div'), { id: 'bad' });
    container.style.cssText = 'width:640px;height:480px;';
    document.body.append(container);
    return (window as any).twip.Editor.mount(container, { document: new Uint8Array([1, 2, 3]) }).then(
      () => 'mounted',
      (e: { code: string }) => e.code,
    );
  });
  expect(code).toBe('import-failed');
  expect(await page.locator('#bad *').count()).toBe(0);
  expect(await resources(page)).toEqual(before);
  // The container can take an editor after all.
  await page.evaluate(() => (window as any).twip.Editor.mount(document.getElementById('bad')).then((e: any) => e.destroy()));
});

test('an aborted mount rejects aborted and leaves nothing', async ({ page }) => {
  const code = await page.evaluate(async (docx) => {
    const container = Object.assign(document.createElement('div'), { id: 'aborted' });
    container.style.cssText = 'width:640px;height:480px;';
    document.body.append(container);
    const controller = new AbortController();
    const mounting = (window as any).twip.Editor.mount(container, { document: new Uint8Array(docx), signal: controller.signal });
    controller.abort();
    return mounting.then(() => 'mounted', (e: { code: string }) => e.code);
  }, hello);
  expect(code).toBe('aborted');
  expect(await page.locator('#aborted *').count()).toBe(0);
});

test('destroy is idempotent, and later calls reject destroyed', async ({ page }) => {
  await mount(page, { docx: hello });
  const codes = await page.evaluate(async () => {
    const editor = (window as any).editor;
    await Promise.all([editor.destroy(), editor.destroy()]);
    await editor.destroy();
    const code = (p: Promise<unknown>) => p.then(() => 'resolved', (e: { code: string }) => e.code);
    return [await code(editor.commands.insertText('x')), await code(editor.export({ format: 'docx' })), await code(editor.wordCount())];
  });
  expect(codes).toEqual(['destroyed', 'destroyed', 'destroyed']);
  expect(await page.locator('#editor *').count()).toBe(0);
});

test('mounting takes no focus, and focus() keeps the selection', async ({ page }) => {
  await mount(page, { docx: hello });
  expect(await page.evaluate(() => document.activeElement === document.body)).toBe(true);
  const [before, after, focused] = await page.evaluate(async () => {
    const editor = (window as any).editor;
    await editor.commands.selectAll();
    const before = editor.getState().selection;
    editor.focus();
    await new Promise((r) => setTimeout(r, 50));
    return [before, editor.getState().selection, editor.container.contains(document.activeElement)];
  });
  expect(after).toEqual(before);
  expect(before.kind).toBe('text');
  expect(focused).toBe(true);
});

test('mount, open, type, export and destroy', async ({ page }) => {
  await mount(page);
  const warnings = await page.evaluate((docx) => (window as any).editor.open(new Uint8Array(docx)).then((r: { warnings: unknown[] }) => r.warnings), hello);
  expect(Array.isArray(warnings)).toBe(true);
  await page.locator('#editor canvas').click({ position: { x: 300, y: 150 } });
  await page.keyboard.press('Control+End');
  await page.keyboard.type(' typed');
  await expect.poll(() => text(page)).toContain(' typed');
  const result = await page.evaluate(async () => {
    const editor = (window as any).editor;
    const docx = await editor.export({ format: 'docx' });
    const pdf = await editor.export({ format: 'pdf' });
    const kept = [...docx.bytes];
    await editor.commands.insertText(' later');
    return { revision: docx.revision, state: editor.getState().revision, warnings: Array.isArray(docx.warnings) && Array.isArray(pdf.warnings), pdf: String.fromCharCode(...pdf.bytes.subarray(0, 5)), docx: kept, same: pdf.revision === docx.revision };
  });
  expect(result.revision).toBeLessThan(result.state);
  expect(result.warnings).toBe(true);
  expect(result.same).toBe(true);
  expect(result.pdf).toBe('%PDF-');
  const exported = docxText(Buffer.from(result.docx));
  expect(exported).toContain(' typed');
  expect(exported).not.toContain(' later');
  await page.evaluate(() => (window as any).editor.destroy());
  expect(await page.locator('#editor *').count()).toBe(0);
});

test('the accessibility DOM mirrors the document, with a polite live region', async ({ page }) => {
  await mount(page, { docx: hello });
  const tree = page.locator('#editor [data-twip="a11y"]');
  await expect(tree.locator('[role="document"]')).toHaveCount(1);
  await expect(tree.locator('[role="paragraph"]').first()).toHaveText('Hello from Twip.');
  await page.evaluate(() => (window as any).editor.commands.insertText('More '));
  await expect(tree).toContainText('More Hello');
  await expect(page.locator('#editor [aria-live="polite"]')).toHaveCount(1);
  await expect(page.locator('#editor textarea')).toHaveAttribute('aria-label', 'Document');
});
