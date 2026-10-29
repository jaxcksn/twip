import { Editor } from '@twip/web';

const container = document.querySelector<HTMLElement>('#editor')!;

const editor = await Editor.mount(container, {
  // A phase of the caret's blink, ms (0: steady), and stop blinking after 10 s without input.
  caretBlink: 600,
  caretBlinkStopAfter: 10_000,
  // Only the colours given change; under forced colours the editor otherwise uses the system's.
  overlayColors: { selection: '#FFD54F' },
  // The memory pages' drawing is kept in, bytes.
  paintBudget: { recordings: 16 << 20 },
  // The editor draws the comment balloons' leader lines.
  drawLeaders: true,
});

// Later: back to a steady caret, and a darker caret.
await editor.view.setCaretBlink(0);
await editor.view.setOverlayColors({ caret: '#1A237E' });

// What the frames have cost: for a benchmark or a test.
const stats = await editor.view.frameStats();
console.log(`${stats.frames} frames, p95 ${stats.p95FrameMs.toFixed(1)} ms, ${stats.pagesReused} pages drawn from recordings`);
