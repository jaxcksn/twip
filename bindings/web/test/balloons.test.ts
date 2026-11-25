// The balloon settings: where a card goes beside its page, and the width kept in range.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { balloonLeft, balloonOptions, defaultBalloonOptions } from '../dist/review.js';

const anchor = { thread: 'c1', page: 1, active: false, done: false, start: { x: 120, y: 300 }, end: { x: 140, y: 300 }, lineTop: 288, lineBottom: 304, pageEdge: 856, pageLeft: 40 } as never;

test('a balloon goes right of its page, or left of it with the left side', () => {
  assert.equal(balloonLeft(anchor, 200, 'right', 8), 864);
  assert.equal(balloonLeft(anchor, 200, 'left', 8), 40 - 8 - 200);
  assert.equal(balloonLeft(anchor, 200, 'right', 8, 900), 900, 'a markup pane on the right');
});

test('the width keeps to its range', () => {
  assert.deepEqual(defaultBalloonOptions, { side: 'right', width: { points: 266.4 }, connectingLines: true, commentColor: 'byAuthor' });
  assert.deepEqual(balloonOptions(defaultBalloonOptions, { width: { percent: 140 } }).width, { percent: 100 });
  assert.deepEqual(balloonOptions(defaultBalloonOptions, { width: { percent: -5 } }).width, { percent: 0 });
  assert.deepEqual(balloonOptions(defaultBalloonOptions, { width: { points: -1 } }).width, { points: 0 });
  assert.equal(balloonOptions(defaultBalloonOptions, { side: 'left' }).side, 'left');
});
