// Every command in the engine's wire fixture has a `commands` method, and each method sends its op exactly as the
// fixture has it: the fixture's argument goes in, the fixture's JSON comes out.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import type { CommandId, Op } from '../dist/index.js';
import { createCommands, fromOp } from '../dist/commands.js';
import { toWire } from '../dist/editor.js';

const fixture = (name: string) => JSON.parse(readFileSync(new URL(`../../../crates/twip/tests/wire/${name}.json`, import.meta.url), 'utf8')) as unknown[];

test('every fixture command has a method that encodes it as the fixture does', async () => {
  let sent: unknown;
  const commands = createCommands(async (op) => (sent = toWire(op))) as unknown as Record<CommandId, (arg?: unknown) => Promise<unknown>>;
  for (const op of fixture('command') as Op[]) {
    const [id, arg] = fromOp(op);
    assert.equal(typeof commands[id], 'function', `no commands.${id}`);
    await commands[id](arg);
    assert.deepEqual(sent, op, `commands.${id}`);
  }
});

test('the methods are exactly the commands the engine has', () => {
  const ids = fixture('command-id') as string[];
  const types = new Set((fixture('command') as Op[]).map((op) => op.type));
  assert.deepEqual([...types].sort(), [...ids].sort(), 'command.json covers every command id');
  assert.deepEqual(Object.keys(createCommands(async () => {})).sort(), [...ids].sort());
});

test('bytes travel as base64', () => {
  assert.deepEqual(toWire({ type: 'insertPicture', data: new Uint8Array([0, 1, 2, 250]) }), { type: 'insertPicture', data: 'AAEC+g==' });
});
