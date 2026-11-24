// The IME's input element follows the caret bar, its text running the way the caret's line does.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { placeInput } from '../dist/host/input.js';

test('the input element lies along a vertical line and back across a horizontal one', () => {
  const input = { style: {} as Record<string, string> } as unknown as HTMLElement;
  placeInput(input, { x: 400, y: 96, width: 20, height: 0, line: 'down' });
  const st = input.style as unknown as Record<string, string>;
  assert.deepEqual([st.width, st.fontSize, st.writingMode, st.height], ['20px', '16px', 'vertical-rl', '']);
  placeInput(input, { x: 400, y: 96, width: 20, height: 0, line: 'up' });
  assert.deepEqual([st.writingMode, st.transform], ['vertical-lr', 'rotate(180deg)']);
  placeInput(input, { x: 96, y: 120, width: 0, height: 20, line: 'horizontal' });
  assert.deepEqual([st.writingMode, st.width, st.height, st.fontSize, st.transform], ['horizontal-tb', '', '20px', '16px', '']);
});
