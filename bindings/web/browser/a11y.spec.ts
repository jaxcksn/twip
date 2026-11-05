// The accessibility DOM a mounted editor puts in its container: the editing element (the text around the caret and the
// selection in it), the structure mirror (the reading tree as ARIA elements, patched by node id), the live region,
// and leaving the editor with Escape then Tab.
import { expect, test } from '@playwright/test';
import { fixture, hello, mount, open, text } from './helpers.ts';

test.beforeEach(({ page }) => open(page));

const input = '#editor [data-twip="input"]';
const tree = '#editor [data-twip="a11y"]';

/** The DOM selection in the editing element, as the text before it and the text selected. */
const selection = (page: import('@playwright/test').Page) =>
  page.evaluate((input) => {
    const e = document.querySelector(input)!;
    const sel = getSelection()!;
    const r = document.createRange();
    r.setStart(e, 0);
    r.setEnd(sel.anchorNode!, sel.anchorOffset);
    return { before: r.toString(), selected: sel.toString() };
  }, input);

test('the editing element is a named, editable, multi-line document that holds the text around the caret', async ({ page }) => {
  await mount(page, { docx: hello });
  const e = page.locator(input);
  await expect(e).toHaveAttribute('role', 'textbox');
  await expect(e).toHaveAttribute('aria-multiline', 'true');
  await expect(e).toHaveAttribute('aria-label', 'Document');
  await expect(e).toHaveAttribute('aria-description', /F6, or Escape then Tab/);
  await expect(e).toHaveAttribute('contenteditable', 'true');
  await expect(page.locator('#editor canvas')).toHaveAttribute('aria-hidden', 'true');
  await e.focus();
  await expect(e).toContainText('Hello from Twip.');
  await page.keyboard.press('End');
  await page.keyboard.type('ab');
  await expect(e).toContainText('Twip.ab');
  await expect.poll(() => selection(page)).toMatchObject({ selected: '' });
  expect((await selection(page)).before.endsWith('ab')).toBe(true);
  await page.keyboard.press('Shift+ArrowLeft');
  await expect.poll(async () => (await selection(page)).selected).toBe('b');
  expect(await text(page)).toContain('Twip.ab');
});

test('a key sent without a code, or as a navigating keypad key, is still the editor’s', async ({ page }) => {
  await mount(page, { docx: hello });
  await page.locator(input).focus();
  // As screen readers and automation send keys: no scan code (Chrome), or no extended flag (Firefox: Numpad7 for Home).
  const prevented = await page.evaluate(
    (input) =>
      [
        { key: 'Home', code: 'Numpad7', keyCode: 36 },
        { key: 'End', code: '', keyCode: 35 },
      ].map((k) => {
        const e = new KeyboardEvent('keydown', { ...k, bubbles: true, cancelable: true });
        document.querySelector(input)!.dispatchEvent(e);
        return e.defaultPrevented;
      }),
    input,
  );
  expect(prevented).toEqual([true, true]);
  await page.keyboard.type('ab');
  await expect(page.locator(input)).toContainText('Twip.ab');
});

test('a wrapped paragraph is one block whose browser lines are the canvas lines', async ({ page }) => {
  await mount(page, { docx: fixture('editor/FX-A11Y-STRUCTURE.docx') });
  // The caret into the long paragraph after the first heading.
  await page.evaluate(async () => {
    const editor = (window as any).editor;
    await editor.accessibility.go((await editor.accessibility.tree()).children[1].id);
  });
  await page.locator(input).focus();
  const lines = await page.evaluate((input) => {
    const p = [...document.querySelectorAll(`${input} > div`)].find((d) => d.querySelectorAll('[data-twip-line]').length > 1)!;
    const spans = [...p.querySelectorAll<HTMLElement>('[data-twip-line]')];
    return {
      br: p.querySelectorAll('br').length,
      // Each canvas line one browser line, below the one before.
      boxes: spans.map((s) => s.getClientRects().length),
      tops: spans.map((s) => s.getClientRects()[0]!.top),
    };
  }, input);
  expect(lines.br).toBe(0);
  expect(lines.boxes.every((n) => n === 1)).toBe(true);
  expect(lines.tops.every((t, i) => i === 0 || t > lines.tops[i - 1]!)).toBe(true);
});

test('Tab from before the editor goes to the editing element: the scroller is no stop', async ({ page }) => {
  await mount(page, { docx: hello });
  await page.evaluate(() => document.querySelector('#editor')!.before(Object.assign(document.createElement('button'), { id: 'before', textContent: 'Before' })));
  await page.locator('#before').focus();
  await page.keyboard.press('Tab');
  await expect(page.locator(input)).toBeFocused();
  // Firefox makes scrollers Tab stops; focus that lands there anyway goes on.
  await expect(page.locator('#editor [data-twip="editor"]')).toHaveAttribute('tabindex', '-1');
  await page.locator('#editor [data-twip="editor"]').focus();
  await expect(page.locator(input)).toBeFocused();
});

test('F6 and Shift+F6 leave the editor for the next and previous stops, as Word’s panes', async ({ page }) => {
  await mount(page, { docx: hello });
  await page.evaluate(() => {
    const editor = document.querySelector('#editor')!;
    editor.before(Object.assign(document.createElement('button'), { id: 'before', textContent: 'Before' }));
    editor.after(Object.assign(document.createElement('button'), { id: 'after', textContent: 'After' }));
  });
  await page.locator(input).focus();
  const before = await text(page);
  await page.keyboard.press('F6');
  await expect(page.locator('#after')).toBeFocused();
  await page.locator(input).focus();
  await page.keyboard.press('Shift+F6');
  await expect(page.locator('#before')).toBeFocused();
  expect(await text(page)).toBe(before);
});

test('Escape then Tab leaves the editor and changes nothing; Tab alone stays', async ({ page }) => {
  await mount(page, { docx: hello });
  await page.evaluate(() => document.querySelector('#editor')!.after(Object.assign(document.createElement('button'), { id: 'after', textContent: 'After' })));
  await page.locator(input).focus();
  await page.keyboard.press('Home');
  const before = await text(page);
  await page.keyboard.press('Tab');
  await expect(page.locator(input)).toBeFocused();
  await page.keyboard.press('Backspace'); // the tab Tab typed
  await page.keyboard.press('Escape');
  await page.keyboard.press('Tab');
  await expect(page.locator('#after')).toBeFocused();
  expect(await text(page)).toBe(before);
});

test('the mirror has the structure, and an edit replaces only the paragraph it changed', async ({ page }) => {
  await mount(page, { docx: fixture('editor/FX-A11Y-STRUCTURE.docx') });
  const t = page.locator(tree);
  await expect(t.locator('[role="document"]')).toHaveAttribute('lang', 'en-US');
  await expect(t.locator('[role="heading"][aria-level="1"]').first()).toHaveText('Introduction');
  await expect(t.locator('[role="list"]').first().locator(':scope > [role="listitem"]')).toHaveCount(3);
  await expect(t.locator('[role="listitem"][aria-posinset="2"][aria-setsize="3"]').first()).toContainText('Second item');
  await expect(t.locator('[role="columnheader"]')).toHaveCount(3);
  await expect(t.locator('[role="cell"][aria-rowspan="2"]')).toHaveText('North');
  await expect(t.locator('[role="img"]')).toHaveCount(1);
  await expect(t.locator('[role="img"]')).toHaveAttribute('aria-label', 'Board seal');
  // The regions after the body (headers and footers are named regions inside the first).
  await expect(t.locator('[role="document"] > [role="region"]')).toHaveCount(4);
  await expect(t.locator('[lang="fr-FR"]')).toContainText('Bonjour');
  await expect(t.locator('[lang="he-IL"]')).toHaveText('שלום לכולם');
  // An edit in the last paragraph: the heading's element stays the same element.
  await page.evaluate((tree) => Object.assign(window, { kept: document.querySelector(`${tree} [role="heading"]`) }), tree);
  await page.evaluate(async () => {
    const editor = (window as any).editor;
    const node = (await editor.accessibility.tree()).children.find((n: any) => n.children?.[0]?.text === 'Last paragraph.');
    await editor.accessibility.go(node.id);
    await editor.commands.insertText('More ');
  });
  await expect(t).toContainText('More Last paragraph.');
  expect(await page.evaluate((tree) => (window as any).kept === document.querySelector(`${tree} [role="heading"]`), tree)).toBe(true);
});

test('clicking a mirror element starts editing there', async ({ page }) => {
  await mount(page, { docx: fixture('editor/FX-A11Y-STRUCTURE.docx') });
  const heading = page.locator(`${tree} [role="heading"]`, { hasText: 'Tables' });
  await heading.dispatchEvent('click');
  await expect(page.locator(input)).toBeFocused();
  await expect(page.locator(input)).toContainText('Tables');
  await expect.poll(() => page.evaluate(() => (window as any).editor.getState().selection.text ?? '')).toBe('');
});

test('a screen reader’s activation (focus, then a mouse click on the canvas) starts editing at the node', async ({ page }) => {
  await mount(page, { docx: fixture('editor/FX-A11Y-STRUCTURE.docx') });
  // Out of focus the editing element is empty: browse mode finds the text once, in the mirror.
  await expect(page.locator(input)).toBeEmpty();
  await page.locator(`${tree} [role="heading"]`, { hasText: 'Tables' }).focus();
  await expect(page.locator(input)).toBeFocused();
  await expect(page.locator(input)).toContainText('Tables');
  // The click that follows lands on the canvas at the node's (clipped) place: it doesn't move the caret.
  await page.mouse.click(5, 5);
  await page.waitForTimeout(100);
  await expect.poll(() => page.evaluate(() => getSelection()!.anchorNode?.parentElement?.closest('[data-twip="input"] > div')?.textContent)).toContain('Tables');
});

test('headers and footers are groups in their region: the landmarks are the regions', async ({ page }) => {
  await mount(page, { docx: fixture('editor/FX-A11Y-STRUCTURE.docx') });
  await expect(page.locator(`${tree} [role="region"] > [role="group"][aria-label^="Header"]`).first()).toBeAttached();
  await expect(page.locator(`${tree} [role="region"] [role="region"]`)).toHaveCount(0);
});

test('editing a sub-template names it in the editing element, and a shown body is a read-only group', async ({ page }) => {
  await mount(page, { docx: fixture('editor/FX-SUBTEMPLATES.docx') });
  await page.evaluate(() => (window as any).editor.commands.setSubTemplateRules({ rules: [{ fieldType: 'SUBTEMPLATE', category: 'Sub-templates' }] }));
  await expect(page.locator(`${tree} [role="group"][aria-label="Sub-template Violation, read only"]`).first()).toContainText('Violation notice');
  await page.evaluate(() => (window as any).editor.subTemplates.edit('Violation'));
  await expect(page.locator(input)).toHaveAttribute('aria-label', 'Document, sub-template Violation');
  await page.evaluate(() => (window as any).editor.subTemplates.close());
  await expect(page.locator(input)).toHaveAttribute('aria-label', 'Document');
});

test('two editors on a page keep their own names, mirrors and live regions', async ({ page }) => {
  await page.evaluate(async (docx) => {
    const { Editor } = (window as any).twip;
    for (const name of ['first', 'second']) {
      const div = Object.assign(document.createElement('div'), { id: name });
      div.style.cssText = 'width:600px;height:300px;';
      document.body.append(div);
      Object.assign(window, { [name]: await Editor.mount(div, { document: new Uint8Array(docx), label: name === 'first' ? 'First' : 'Second' }) });
    }
  }, hello);
  await expect(page.locator('#first [data-twip="input"]')).toHaveAttribute('aria-label', 'First');
  await expect(page.locator('#second [data-twip="input"]')).toHaveAttribute('aria-label', 'Second');
  await expect(page.locator('#first [role="document"]')).toHaveAttribute('aria-label', 'First');
  await page.evaluate(() => (window as any).first.commands.describeFormatting());
  await expect(page.locator('#first [aria-live="polite"]')).not.toBeEmpty();
  await expect(page.locator('#second [aria-live="polite"]')).toBeEmpty();
  // Element ids are distinct across the two mirrors.
  const ids = await page.evaluate(() => [...document.querySelectorAll('[data-twip="a11y"] [id]')].map((e) => e.id));
  expect(new Set(ids).size).toBe(ids.length);
});

test('nothing with data-twip is left after destroy', async ({ page }) => {
  await mount(page, { docx: hello });
  await page.evaluate(() => (window as any).editor.destroy());
  expect(await page.locator('[data-twip]').count()).toBe(0);
});
