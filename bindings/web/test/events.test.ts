// Event dispatch on a headless editor: isolation, ordering against the state cache, unsubscribing and destroying.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { EditorError, HeadlessEditor, type TransactionRecord } from '../dist/index.js';

test('a throwing listener doesn’t stop the others, and its exception arrives on error', async () => {
  const editor = await HeadlessEditor.create();
  const errors: EditorError[] = [];
  let after = 0;
  editor.on('error', (e) => errors.push(e));
  editor.on('documentChanged', () => {
    throw new Error('application bug');
  });
  editor.on('documentChanged', () => after++);
  await editor.commands.insertText('a');
  assert.equal(after, 1);
  assert.equal(errors.length, 1);
  assert.ok(errors[0] instanceof EditorError && (errors[0].cause as Error).message === 'application bug');
  await editor.destroy();
});

test('with no error listener, a listener’s exception goes to console.error', async (t) => {
  const editor = await HeadlessEditor.create();
  const logged = t.mock.method(console, 'error', () => {});
  editor.on('documentChanged', () => {
    throw new Error('application bug');
  });
  await editor.commands.insertText('a');
  assert.equal(logged.mock.callCount(), 1);
  await editor.destroy();
});

test('getState() inside a listener already shows the event’s revision', async () => {
  const editor = await HeadlessEditor.create();
  const seen: [number, number][] = [];
  editor.on('documentChanged', (record: TransactionRecord) => seen.push([record.revision, editor.getState().revision]));
  await editor.commands.insertText('a');
  await editor.commands.insertText('b');
  assert.equal(seen.length, 2);
  for (const [event, state] of seen) assert.equal(state, event);
  await editor.destroy();
});

test('a command from a listener runs after the current dispatch', async () => {
  const editor = await HeadlessEditor.create();
  const order: string[] = [];
  let queued: Promise<void> | undefined;
  editor.on('documentChanged', (record) => {
    order.push(`changed ${record.revision}`);
    if (!queued) queued = editor.commands.insertText('b').then(() => void order.push('listener command done'));
    order.push(`dispatched ${record.revision}`);
  });
  const first = editor.getState().revision + 1;
  await editor.commands.insertText('a');
  await queued;
  assert.deepEqual(order, [`changed ${first}`, `dispatched ${first}`, `changed ${first + 1}`, `dispatched ${first + 1}`, 'listener command done']);
  await editor.destroy();
});

test('unsubscribing stops a listener', async () => {
  const editor = await HeadlessEditor.create();
  let calls = 0;
  let states = 0;
  const off = editor.on('documentChanged', () => calls++);
  const stop = editor.subscribe(() => states++);
  assert.equal(states, 1, 'subscribe calls the listener at once');
  await editor.commands.insertText('a');
  off();
  stop();
  const seen = states;
  await editor.commands.insertText('b');
  assert.equal(calls, 1);
  assert.equal(states, seen);
  await editor.destroy();
});

test('destroy() inside a listener stops further dispatch cleanly', async () => {
  const editor = await HeadlessEditor.create();
  let later = 0;
  let destroying: Promise<void> | undefined;
  editor.on('documentChanged', () => {
    destroying = editor.destroy();
  });
  editor.on('documentChanged', () => later++);
  await editor.commands.insertText('a');
  await destroying;
  assert.equal(later, 0);
  await assert.rejects(editor.commands.insertText('b'), (e) => e instanceof EditorError && e.code === 'destroyed');
  await editor.destroy();
});
