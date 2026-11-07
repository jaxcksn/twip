// The page's font library against a stand-in engine: library calls keep their order, and an engine font request goes
// to the resolver, whose answer (a font, a family, a category, none, or a failure) goes back to the engine.
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { test } from 'node:test';
import { FontLibrary, type FaceInfo } from '../dist/index.js';
import { answerFontRequest, useEngine } from '../dist/fonts/library.js';
import { Kind, Method, type Message } from '../dist/transport/protocol.js';

const calls: [number, unknown][] = [];
const added: Uint8Array[] = [];
let faces: FaceInfo[] = [];
const listeners = new Map<number, (message: Message) => void>();
useEngine({
  call: async (_session: number, method: number, request: unknown) => void calls.push([method, request ?? null]),
  addFont: async (bytes: Uint8Array) => (added.push(bytes), faces),
  listen: (session: number, listener: (message: Message) => void) => (listeners.set(session, listener), () => {}),
} as never);
const library = FontLibrary.default();
const reset = () => {
  calls.length = 0;
  added.length = 0;
};
const request = { family: 'Roboto Slab', bold: false, italic: false };

test('one library per page, and its settings reach the engine in call order', () => {
  assert.equal(FontLibrary.default(), library);
  reset();
  library.setReplacements({ 'Times New Roman': 'Tinos' });
  library.setReplacements([{ from: 'Arial', bold: true, to: 'Arimo' }]);
  library.setFallback('Tinos');
  library.setAliases({ Arial: 'Arimo' });
  library.setAliases([{ from: 'Garamond', to: { category: 'serif' }, bold: true }]);
  library.setGenericDefaults({ serif: 'Tinos', monospace: 'Cousine' });
  library.setClassification({ fromDocument: false });
  library.setCharacterFallback(['Noto Sans Symbols', 'Noto Sans CJK']);
  assert.deepEqual(calls, [
    [Method.FontSetReplacements, [{ from: 'Times New Roman', to: 'Tinos' }]],
    [Method.FontSetReplacements, [{ from: 'Arial', bold: true, to: 'Arimo' }]],
    [Method.FontSetFallback, 'Tinos'],
    [Method.FontSetAliases, [{ from: 'Arial', to: 'Arimo' }]],
    [Method.FontSetAliases, [{ from: 'Garamond', to: { category: 'serif' }, bold: true }]],
    [Method.FontSetGenericDefaults, { serif: 'Tinos', monospace: 'Cousine' }],
    [Method.FontSetClassification, false],
    [Method.FontSetCharacterFallback, ['Noto Sans Symbols', 'Noto Sans CJK']],
  ]);
});

test('a request goes to the resolver, and its answer back to the engine', async () => {
  reset();
  const asked: unknown[] = [];
  library.setResolver(async (r) => (asked.push(r), r.family === 'Roboto Slab' ? new Uint8Array([1]) : null));
  assert.deepEqual(calls, [[Method.FontSetResolver, true]], 'with a resolver, the engine asks instead of calling a family missing');
  reset();
  faces = [{ family: 'roboto slab', bold: false, italic: false }];
  listeners.get(0)!({ kind: Kind.FontRequest, sessionId: 0, payload: request });
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual([asked, added.length], [[request], 1], 'the engine’s request (session 0) reaches the resolver');
  assert.deepEqual(calls, [[Method.FontAnswer, { request, answer: { kind: 'font' } }]], 'the font is added, then answered');
  reset();
  const unknown = { ...request, family: 'Zzyzx Unknown' };
  await answerFontRequest(unknown);
  assert.deepEqual(calls, [[Method.FontAnswer, { request: unknown, answer: { kind: 'missing' } }]]);
});

test('a resolver answers with a family or a category', async () => {
  library.setResolver(async (r) => (r.family === 'Garamond' ? { category: 'serif' } : { family: 'Arimo' }));
  reset();
  const garamond = { family: 'Garamond', bold: true, italic: false, proposed: 'serif' as const, description: { class: 'roman' as const } };
  await answerFontRequest(garamond);
  await answerFontRequest(request);
  assert.deepEqual(calls, [
    [Method.FontAnswer, { request: garamond, answer: { kind: 'category', category: 'serif' } }],
    [Method.FontAnswer, { request, answer: { kind: 'family', family: 'Arimo' } }],
  ]);
});

test('a resolver that throws answers failed with the error, logging nothing', async (t) => {
  library.setResolver(async () => {
    throw new Error('offline');
  });
  const logged = t.mock.method(console, 'error', () => {});
  reset();
  const offline = { ...request, family: 'Offline' };
  await answerFontRequest(offline);
  assert.deepEqual(calls, [[Method.FontAnswer, { request: offline, answer: { kind: 'failed', message: 'offline' } }]]);
  assert.equal(logged.mock.callCount(), 0);
});

test('the package contacts no font service', () => {
  const dist = new URL('../dist/', import.meta.url);
  const files = readdirSync(dist, { recursive: true, withFileTypes: true }).filter((f) => f.isFile());
  for (const f of files) {
    const text = readFileSync(`${f.parentPath}/${f.name}`).toString('latin1');
    for (const host of ['fonts.googleapis.com', 'fonts.gstatic.com']) assert.ok(!text.includes(host), `${f.name} names ${host}`);
  }
});
