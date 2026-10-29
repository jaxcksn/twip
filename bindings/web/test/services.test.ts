// The services' engine calls, against a stand-in transport: each method sends its method number and the request in
// the engine's JSON shape (the wire fixtures), and commits resolve only once the state cache holds their revision.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import type { CommentAnchor, DialogKind, DialogValues, EditorError, EditorState, FieldInfo, MenuAction } from '../dist/index.js';
import { Session } from '../dist/editor.js';
import { createReview, stackBalloons } from '../dist/review.js';
import { createShortcuts } from '../dist/services/accessibility.js';
import { createDialogs } from '../dist/services/dialogs.js';
import { createDataParts, createElements } from '../dist/services/elements.js';
import { createFind } from '../dist/services/find.js';
import { createMenus } from '../dist/services/menus.js';
import { createStyles } from '../dist/services/navigation.js';
import { createSpelling } from '../dist/services/spelling.js';
import { Kind, Method, type Message } from '../dist/transport/protocol.js';

const fixture = <T>(name: string) => (JSON.parse(readFileSync(new URL(`../../../crates/twip/tests/wire/${name}.json`, import.meta.url), 'utf8')) as T[])[0]!;
const state = fixture<EditorState>('state');

/** Records every call; answers each method with what `answers` holds for it, else null. */
class StandIn {
  calls: [number, unknown][] = [];
  answers = new Map<number, unknown>();
  receive: (message: Message) => void = () => {};
  listen(_session: number, listener: (message: Message) => void) {
    this.receive = listener;
    return () => {};
  }
  async call(_session: number, method: number, request: unknown) {
    this.calls.push([method, JSON.parse(JSON.stringify(request ?? null))]);
    return this.answers.get(method) ?? null;
  }
  async closeSession() {}
}

function session(catalog: unknown[] = []): [Session, StandIn] {
  const t = new StandIn();
  const make = Session as unknown as new (...args: unknown[]) => Session;
  return [new make(t, 1, structuredClone(state), catalog), t];
}

test('find, spelling, styles, elements and data parts send the engine’s shapes', async () => {
  const [s, t] = session();
  const query = fixture<{ text: string }>('find-query');
  const find = createFind(s);
  await find.next(query);
  await find.previous(query);
  await find.replaceAll(query, 'sum');
  await createSpelling(s).addDictionary({ language: 'en-US', aff: new TextEncoder().encode('SET UTF-8'), dic: new TextEncoder().encode('1\nhello') });
  await createSpelling(s).suggestions('teh');
  await createStyles(s).list();
  await createElements(s).select('field-1' as never);
  await createElements(s).placeholders();
  assert.deepEqual(t.calls, [
    [Method.Find, { query, direction: 'next' }],
    [Method.Find, { query, direction: 'previous' }],
    [Method.ReplaceAll, { query, with: 'sum' }],
    [Method.SpellingAddDictionary, fixture('dictionary')],
    [Method.SpellingSuggestions, { word: 'teh' }],
    [Method.Styles, null],
    [Method.SelectElement, { id: 'field-1', reveal: false }],
    [Method.Placeholders, null],
  ]);
  t.answers.set(Method.DataParts, [fixture('data-part')]);
  assert.deepEqual(await createDataParts(s).list(), [{ namespace: null, itemId: '{6A9C}' }]);
});

test('a commit resolves once the state cache holds its revision', async () => {
  const [s, t] = session();
  const revision = state.revision + 1;
  t.answers.set(Method.Invoke, { record: { revision }, value: null });
  let done = false;
  const invoked = createMenus(s).invoke(fixture<MenuAction>('menu-action')).then(() => (done = true));
  await new Promise((r) => setTimeout(r, 10));
  assert.equal(done, false);
  t.receive({ kind: Kind.State, sessionId: 1, payload: { ...state, revision } });
  await invoked;
  assert.equal(s.state.revision, revision);
  assert.deepEqual(t.calls, [[Method.Invoke, fixture('menu-action')]]);
});

test('a dialog updates with its kind-tagged patch and applies its model', async () => {
  const [s, t] = session();
  const model = fixture<{ kind: 'insertTable'; values: DialogValues['insertTable'] }>('dialog-model');
  t.answers.set(Method.DialogOpen, model);
  const dialog = await createDialogs(s).open('insertTable');
  assert.equal(dialog.values.rows, 2);
  const changed = { ...model, values: { ...model.values, rows: 3 } };
  t.answers.set(Method.DialogUpdate, changed);
  const updated = await dialog.update({ rows: 3 });
  assert.equal(updated.values.rows, 3);
  dialog.cancel();
  await updated.apply();
  assert.deepEqual(t.calls, [
    [Method.DialogOpen, 'insertTable'],
    [Method.DialogUpdate, { model, patch: { rows: 3, kind: 'insertTable' } }],
    [Method.DialogApply, changed],
  ]);
});

test('the accessibility dialog selects an issue’s object', async () => {
  const [s, t] = session();
  t.answers.set(Method.DialogOpen, { kind: 'accessibility', values: { kind: 'accessibility', issues: [] }, fields: {} });
  const dialog = await createDialogs(s).open('accessibility');
  await dialog.select('picture:3:0');
  assert.deepEqual(t.calls.at(-1), [Method.DialogSelectIssue, 'picture:3:0']);
});

test('a dialog of a kind known at run time is drawn and updated by field name', async () => {
  const [s, t] = session();
  const model = fixture<{ kind: 'insertTable'; values: DialogValues['insertTable'] }>('dialog-model');
  t.answers.set(Method.DialogOpen, model);
  t.answers.set(Method.DialogUpdate, model);
  // What a host that draws every dialog the same way writes: no casts.
  const draw = async (kind: DialogKind) => {
    let dialog = await createDialogs(s).open(kind);
    const values: Readonly<Record<string, unknown>> = dialog.values;
    const fields: [string, FieldInfo][] = Object.entries(dialog.fields);
    for (const [key, info] of fields) if (info.enabled) dialog = await dialog.update({ [key]: values[key] });
    if (dialog.kind === 'accessibility') await dialog.select('picture:3:0');
    return fields.length;
  };
  assert.equal(await draw(model.kind), 1);
  assert.deepEqual(t.calls.at(-1), [Method.DialogUpdate, { model, patch: { rows: model.values.rows, kind: 'insertTable' } }]);
});

test('a fault rejects later operations, and destroy() still works', async () => {
  const [s, t] = session();
  const errors: EditorError[] = [];
  s.on('error', (e) => errors.push(e));
  t.receive({ kind: Kind.Event, sessionId: 1, payload: { type: 'error', code: 'internal', message: 'the engine failed', fatal: true } });
  assert.equal(errors[0]?.fatal, true);
  await assert.rejects(createFind(s).next({ text: 'x' }), (e: EditorError) => e.code === 'internal' && e.fatal);
  assert.equal(t.calls.length, 0);
  await s.destroy();
});

test('balloons stack as the engine stacks them, in every case of balloons.json', () => {
  type Case = { name: string; anchors: CommentAnchor[]; heights: number[]; gap: number; tops: number[] };
  const cases = JSON.parse(readFileSync(new URL('../../../crates/twip/tests/wire/balloons.json', import.meta.url), 'utf8')) as Case[];
  assert.ok(cases.length > 0);
  for (const c of cases) assert.deepEqual(stackBalloons(c.anchors, c.heights, c.gap), c.tops, c.name);
  const at = (lineTop: number) => ({ ...fixture<CommentAnchor>('comment-anchor'), lineTop });
  assert.deepEqual(stackBalloons([at(10), at(20), at(200)], [50, 30, 10], 4), [10, 64, 200]);
  const [s] = session();
  assert.deepEqual(createReview(s).stackBalloons([at(10), at(20)], [50, 30]), [10, 68]);
});

test('shortcuts: lookups are synchronous, and a key that is taken fails unavailable, naming it', async () => {
  const [s, t] = session([{ ...fixture<object>('command-info'), id: 'setBold', shortcuts: { windows: ['Ctrl+B'], mac: ['⌘B'] } }]);
  const shortcuts = createShortcuts(s);
  assert.deepEqual(shortcuts.lookup('ctrl+b'), { kind: 'command', id: 'setBold' });
  assert.deepEqual(shortcuts.lookup('Ctrl+S'), { kind: 'host', command: 'save' });
  assert.equal(shortcuts.lookup('Ctrl+Alt+S'), null);
  const off = shortcuts.register({ id: 'insertSignature', keys: { windows: 'Ctrl+Alt+S', mac: '⌥⌘S' } });
  assert.deepEqual(shortcuts.lookup('Alt+Ctrl+S'), { kind: 'shortcut', id: 'insertSignature' });
  assert.deepEqual(shortcuts.lookup('⌘⌥S'), { kind: 'shortcut', id: 'insertSignature' });
  assert.throws(() => shortcuts.register({ id: 'other', keys: '⌘B' }), (e: EditorError) => e.code === 'unavailable' && /setBold/.test(e.message));
  off();
  assert.equal(shortcuts.lookup('Ctrl+Alt+S'), null);
  await new Promise((r) => setTimeout(r));
  assert.deepEqual(t.calls, [
    [Method.ShortcutsRegister, fixture('shortcut')],
    [Method.ShortcutsUnregister, 'insertSignature'],
  ]);
});
