// preload() in Node: the main-thread engine, loaded once and shared with the editors and the font library.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { configure, EditorError, FontLibrary, HeadlessEditor, preload } from '@twip/web';

test('preload loads the engine once, ahead of the first editor', async () => {
  const first = preload();
  assert.equal(preload({ execution: 'main' }), first, "Node has no worker: 'auto' is the main thread, and both share one load");
  await first;
  await assert.rejects(preload({ execution: 'worker' }), (e: unknown) => e instanceof EditorError && e.code === 'unsupported');
  assert.throws(
    () => configure({ assets: 'file:///elsewhere/' }),
    (e: unknown) => e instanceof EditorError && e.code === 'not-allowed',
    'the engine has loaded: the folder is fixed',
  );
  assert.ok(Array.isArray(await FontLibrary.default().families()), 'the font library uses the preloaded engine');
  const editor = await HeadlessEditor.create();
  await editor.open(readFileSync(new URL('../../hello.docx', import.meta.url)));
  assert.ok(editor.getState().revision > 0);
  await editor.destroy();
});
