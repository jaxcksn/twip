// The package as an application imports it in Node (its `node` export condition), with no DOM: a headless editor
// opens, runs commands, reads state, receives events and exports.
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { test } from 'node:test';
import { EditorError, FontLibrary, HeadlessEditor, type TransactionRecord } from '@twip/web';
import { toOp } from '../dist/commands.js';

test('a headless editor in Node', async () => {
  // Node has no system fonts for the engine: give it one, if this machine has one.
  const font = ['C:/Windows/Fonts/calibri.ttf', '/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf'].find(existsSync);
  if (font) await FontLibrary.default().add(readFileSync(font));

  const editor = await HeadlessEditor.create();
  const changes: TransactionRecord[] = [];
  editor.on('documentChanged', (record) => changes.push(record));
  await editor.open(readFileSync(new URL('../../hello.docx', import.meta.url)));
  const opened = editor.getState().revision;

  await editor.commands.insertText('Typed through the SDK. ');
  assert.equal(editor.getState().revision, opened + 1);
  assert.deepEqual(changes.map((c) => [c.origin, c.revision]), [['api', opened + 1]]);

  // A catalog entry without arguments runs as `commands[id]()`: the catalog and the methods agree on which those are.
  for (const entry of editor.catalog) {
    assert.equal(Object.keys(toOp(entry.id, { x: 1 })).length === 1, !entry.takesArguments, entry.id);
  }
  const selectAll = editor.catalog.filter((entry) => !entry.takesArguments).find((entry) => entry.label === 'Select All');
  assert.ok(selectAll);
  await editor.commands[selectAll.id]();
  await editor.commands.setBold(true);
  assert.equal(editor.getState().formatting.bold, true);
  await editor.commands.undo();
  assert.equal(editor.getState().history.canRedo, true);
  await editor.commands.redo();

  const { bytes, revision } = await editor.export({ format: 'docx' });
  assert.ok(bytes.length > 100 && bytes[0] === 0x50, `export: ${bytes.length} bytes`);
  assert.equal(revision, editor.getState().revision);

  await assert.rejects(editor.open(new Uint8Array([1, 2, 3])), (e) => e instanceof EditorError && e.code === 'import-failed');
  assert.equal(editor.getState().revision, revision, 'a failed open keeps the document');

  const abort = new AbortController();
  const opening = editor.open(readFileSync(new URL('../../hello.docx', import.meta.url)), { signal: abort.signal });
  abort.abort();
  await assert.rejects(opening, (e) => e instanceof EditorError && e.code === 'aborted');
  assert.equal(editor.getState().revision, revision, 'an open aborted during the read keeps the document');
  await editor.destroy();
  await editor.destroy();
  await assert.rejects(editor.commands.undo(), (e) => e instanceof EditorError && e.code === 'destroyed');
});
