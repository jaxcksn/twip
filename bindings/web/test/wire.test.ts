// The engine's wire fixtures round-trip through the SDK's public types: every example decodes into its TypeScript
// shape and encodes back to the same JSON. A changed Rust type changes a fixture, and this test catches the SDK
// that didn't follow. Types are erased here: wire-types.ts checks each example against its type at compile time.
import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { test } from 'node:test';
import type { Op } from '../dist/index.js';
import { fromOp, toOp } from '../dist/commands.js';
import { decodeEvent, encodeEvent, fromError, toError } from '../dist/transport/protocol.js';

const wire = new URL('../../../crates/twip/tests/wire/', import.meta.url);

/** Decoding and encoding per fixture: the public value, then the wire JSON again. Plain data is its own shape. */
const codecs: Record<string, (value: never) => unknown> = {
  command: (op: Op) => toOp(...fromOp(op)),
  event: (event: unknown) => {
    const { type, event: payload } = decodeEvent(event);
    return encodeEvent(type, payload);
  },
  error: (error: Parameters<typeof toError>[0]) => fromError(toError(error)),
};

const fixtures = existsSync(wire) ? readdirSync(wire).filter((f) => f.endsWith('.json')) : [];

test('wire fixtures round-trip through the public types', { skip: fixtures.length ? false : `no fixtures in ${wire.pathname}` }, (t) => {
  for (const file of fixtures) {
    const stem = file.replace(/\.json$/, '').toLowerCase();
    const json = JSON.parse(readFileSync(new URL(file, wire), 'utf8')) as unknown;
    const examples = Array.isArray(json) ? json : [json];
    const codec = codecs[stem] ?? ((value: unknown) => JSON.parse(JSON.stringify(value)));
    examples.forEach((example, i) => {
      assert.deepStrictEqual(JSON.parse(JSON.stringify(codec(example as never))), example, `${file}, example ${i}`);
    });
    t.diagnostic(`${file}: ${examples.length} example(s)`);
  }
});

test('ops put a single argument in value and spread an options object', () => {
  const ops: Op[] = [
    { type: 'undo' },
    { type: 'setBold', value: true },
    { type: 'insertText', value: 'Hi' },
    { type: 'insertTable', rows: 2, columns: 3 },
    { type: 'setLineSpacing', rule: 'multiple', lines: 1.5 },
    { type: 'setTableWidth', kind: 'percent', value: 50 },
    { type: 'setRowHeight', value: 'auto' },
    { type: 'addComment', value: 'Looks good' },
    { type: 'paste', as: 'text' },
  ];
  // @ts-expect-error an options object is spread, never put in value
  const wrong: Op = { type: 'insertTable', value: { rows: 2, columns: 3 } };
  assert.ok(wrong);
  for (const op of ops) assert.deepEqual(toOp(...fromOp(op)), op);
  assert.deepEqual(toOp('paste', undefined), { type: 'paste' });
});
