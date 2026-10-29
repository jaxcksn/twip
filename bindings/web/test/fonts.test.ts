// The page's font library against a stand-in engine: library calls keep their order, and an engine font request goes
// to the resolver, then Google Fonts (with fetch stubbed), and is marked unresolved when neither has the family.
import assert from 'node:assert/strict';
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
  library.substitute({ 'Times New Roman': 'Tinos' });
  library.substitute([{ from: 'Arial', bold: true, to: 'Arimo' }]);
  library.setDefault('Tinos');
  assert.deepEqual(calls, [
    [Method.FontSetSubstitutions, [{ from: 'Times New Roman', to: 'Tinos' }]],
    [Method.FontSetSubstitutions, [{ from: 'Arial', bold: true, to: 'Arimo' }]],
    [Method.FontSetDefault, 'Tinos'],
  ]);
});

test('a request goes to the resolver; a miss is marked unresolved', async () => {
  reset();
  const asked: unknown[] = [];
  library.setResolver(async (r) => (asked.push(r), r.family === 'Roboto Slab' ? new Uint8Array([1]) : null));
  assert.deepEqual(calls, [[Method.FontSetResolver, true]], 'with a resolver, the engine asks instead of calling a family unresolved');
  reset();
  faces = [{ family: 'roboto slab', bold: false, italic: false }];
  listeners.get(0)!({ kind: Kind.FontRequest, sessionId: 0, payload: request });
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual([asked, added.length, calls], [[request], 1, []], 'the engine’s request (session 0) reaches the resolver');
  await answerFontRequest({ ...request, family: 'Zzyzx Unknown' });
  assert.deepEqual(calls, [[Method.FontMarkUnresolved, { ...request, family: 'Zzyzx Unknown' }]]);
});

test('Google Fonts: the stylesheet’s files are added, each family fetched once, and an unknown family is unresolved', async (t) => {
  reset();
  library.setResolver(async () => null);
  library.useGoogleFonts();
  reset();
  const fetched: string[] = [];
  t.mock.method(globalThis, 'fetch', async (url: string) => {
    fetched.push(String(url));
    if (String(url).includes('Zzyzx')) return new Response('', { status: 400 });
    if (String(url).includes('ital')) return new Response('', { status: 400 }); // no italics
    if (String(url).startsWith('https://fonts.googleapis.com/')) {
      return new Response("@font-face { src: url(https://fonts.gstatic.com/a.ttf) format('truetype'); }\n@font-face { src: url('https://fonts.gstatic.com/b.ttf'); }");
    }
    return new Response(new Uint8Array([0, 1, 0, 0]));
  });
  await answerFontRequest(request);
  await answerFontRequest({ ...request, bold: true });
  assert.deepEqual(fetched, [
    'https://fonts.googleapis.com/css2?family=Roboto+Slab:ital,wght@0,400;0,700;1,400;1,700',
    'https://fonts.googleapis.com/css2?family=Roboto+Slab:wght@400;700',
    'https://fonts.gstatic.com/a.ttf',
    'https://fonts.gstatic.com/b.ttf',
  ]);
  assert.deepEqual([added.length, calls], [2, []]);
  fetched.length = 0;
  await answerFontRequest({ ...request, family: 'Zzyzx Unknown' });
  assert.equal(fetched.length, 3);
  assert.deepEqual(calls, [[Method.FontMarkUnresolved, { ...request, family: 'Zzyzx Unknown' }]]);
});

test('Google Fonts: a family the browser can’t fetch (no CORS on the 400) is unresolved, with nothing logged', async (t) => {
  reset();
  const fetched = t.mock.method(globalThis, 'fetch', async () => {
    throw new TypeError('Failed to fetch');
  });
  const logged = t.mock.method(console, 'error', () => {});
  await answerFontRequest({ ...request, family: 'Calibri' });
  assert.equal(fetched.mock.callCount(), 1);
  assert.equal(logged.mock.callCount(), 0);
  assert.deepEqual(calls, [[Method.FontMarkUnresolved, { ...request, family: 'Calibri' }]]);
});

test('useGoogleFonts({ families }) asks Google only for those', async (t) => {
  reset();
  library.useGoogleFonts({ families: ['Tinos'] });
  assert.deepEqual(calls, [[Method.FontSetResolver, true]]);
  reset();
  const fetched = t.mock.method(globalThis, 'fetch', async () => new Response('', { status: 500 }));
  await answerFontRequest(request);
  assert.equal(fetched.mock.callCount(), 0);
  assert.deepEqual(calls, [[Method.FontMarkUnresolved, request]]);
});
