// A toolbar outside the editor: clicking its button takes the focus away, and the selection stays for the command.
import { expect, test } from '@playwright/test';
import { docxXml, mount, open, text } from './helpers.ts';

test('a button outside the editor makes the selection bold and keeps it', async ({ page }) => {
  await open(page);
  await mount(page);
  await page.evaluate(() => {
    const button = Object.assign(document.createElement('button'), { id: 'bold', textContent: 'Bold' });
    const w = window as any;
    button.addEventListener('click', () => {
      const editor = w.editor;
      let committed = 0;
      const off = editor.on('documentChanged', (r: { revision: number }) => (committed = r.revision));
      w.result = editor.commands.setBold(true).then(() => {
        off();
        const state = editor.getState();
        return { revision: state.revision, committed, bold: state.formatting.bold, selection: state.selection };
      });
    });
    document.body.append(button);
  });
  await page.locator('#editor canvas').click({ position: { x: 300, y: 120 } });
  await page.keyboard.type('plain bold');
  await page.keyboard.press('Control+Shift+ArrowLeft');
  await expect.poll(() => text(page)).toBe('plain bold');
  const before = await page.evaluate(() => (window as any).editor.getState().selection);
  expect(before.kind).toBe('text');
  await page.locator('#bold').click();
  const result = await page.evaluate(() => (window as any).result);
  expect(result.bold).toBe(true);
  expect(result.revision).toBe(result.committed);
  expect(result.selection).toEqual(before);
  // Only the selected word went bold: a run's properties, then its text.
  const docx = await page.evaluate(async () => [...(await (window as any).editor.export({ format: 'docx' })).bytes]);
  const run = (word: string) => new RegExp(`<w:r>(?:(?!</w:r>).)*<w:b/>(?:(?!</w:r>).)*>${word}</w:t>`, 's');
  const xml = docxXml(Buffer.from(docx));
  expect(xml).toMatch(run('bold'));
  expect(xml).not.toMatch(run('plain ?'));
});
