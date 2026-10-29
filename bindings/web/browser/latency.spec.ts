// Latency at 60 Hz, at devicePixelRatio 2: a subscribed toolbar sees the state of a keystroke, an undo and a caret
// move within one refresh (16.7 ms) of the commit, and a keystroke's frame is painted within one refresh of it, on
// WebGPU, WebGL and the CPU path. The commit comes after the key's event, so each time runs from the event's
// timestamp: an upper bound. Each key is pressed alone, once the last one's frame is painted, and the 95th
// percentile of the samples must be in budget; the numbers are printed either way.
import { expect, test, type Page } from '@playwright/test';
import { hello, mount, open } from './helpers.ts';

const REFRESH = 1000 / 60;
const SAMPLES = 20;

test.use({ deviceScaleFactor: 2 });

/** The editor's canvas backend, read from the context it holds. */
function backend(page: Page): Promise<string> {
  return page.evaluate(() => {
    const canvas = document.querySelector('#editor canvas') as HTMLCanvasElement;
    // A canvas gives only the kind of context it already has.
    if (canvas.getContext('webgpu')) return 'webgpu';
    if (canvas.getContext('webgl2')) return 'webgl';
    return canvas.getContext('2d') ? 'cpu' : 'none';
  });
}

/** Times each keydown to the first state after it, and to the first frame drawn after that state. */
async function instrument(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const { sessionOf } = await import('/web/dist/editor.js' as string);
    const w = window as any;
    const editor = w.editor;
    document.addEventListener('keydown', (e) => {
      const sample: any = { key: e.timeStamp, state: 0, frame: 0 };
      sample.seen = new Promise((r) => (sample.see = r));
      sample.painted = new Promise((r) => (sample.paint = r));
      w.sample = sample;
    }, true);
    // A caret move's state has the selection's kind, not its place: any new state is the key's.
    editor.subscribe(() => {
      if (w.sample && !w.sample.state) {
        w.sample.state = performance.now();
        w.sample.see();
      }
    });
    // The frame loop's draw: only a frame asked for after the state can show it.
    const transport = sessionOf(editor).transport;
    const frame = transport.frame.bind(transport);
    transport.frame = (...args: unknown[]) => {
      const sample = w.sample?.state ? w.sample : undefined;
      const start = performance.now();
      return frame(...args).then((drawn: unknown) => {
        if (drawn && sample && !sample.frame) {
          sample.frame = performance.now();
          sample.draw = sample.frame - start;
          sample.paint();
        }
        return drawn;
      });
    };
  });
}

/** Presses `key` `SAMPLES` times, each once its state is seen (and its frame painted, with `paint`); each press's
 * keydown-to-state and keydown-to-frame times, in ms (Infinity for one that didn't come within 2 s), and how long
 * its frame took to draw. */
async function measure(page: Page, key: string, paint = false): Promise<{ state: number[]; frame: number[]; draw: number[] }> {
  const state: number[] = [];
  const frame: number[] = [];
  const draw: number[] = [];
  for (let i = 0; i < SAMPLES; i++) {
    await page.keyboard.press(key);
    const s = await page.evaluate(async (paint) => {
      const sample = (window as any).sample;
      await Promise.race([paint ? sample.painted : sample.seen, new Promise((r) => setTimeout(r, 2000))]);
      const since = (t: number) => (t ? t - sample.key : Infinity);
      return { state: since(sample.state), frame: since(sample.frame), draw: sample.draw ?? Infinity };
    }, paint);
    state.push(s.state);
    frame.push(s.frame);
    draw.push(s.draw);
  }
  return { state, frame, draw };
}

/** The 95th percentile, and a line of the numbers for the test's output. */
function p95(name: string, times: number[]): number {
  const sorted = [...times].sort((a, b) => a - b);
  const at = (q: number) => sorted[Math.min(sorted.length - 1, Math.ceil(q * sorted.length) - 1)]!;
  console.log(`${name}: p50 ${at(0.5).toFixed(1)} ms, p95 ${at(0.95).toFixed(1)} ms, max ${sorted.at(-1)!.toFixed(1)} ms (budget ${REFRESH.toFixed(1)})`);
  return at(0.95);
}

/** A mounted editor with the caret at the end of the document's first paragraph, instrumented. */
async function ready(page: Page, execution: 'main' | 'worker'): Promise<void> {
  await mount(page, { docx: hello, mount: { execution } });
  await page.locator('#editor canvas').click({ position: { x: 300, y: 120 } });
  await page.keyboard.press('End');
  await instrument(page);
}

for (const execution of ['main', 'worker'] as const) {
  test(`a subscribed toolbar sees a keystroke, an undo and a caret move within a refresh (${execution})`, async ({ page }) => {
    await open(page);
    await ready(page, execution);
    const typed = await measure(page, 'a');
    const moved = await measure(page, 'ArrowLeft');
    // Typing joins one undo step and each Backspace is its own: delete the typing, then undo each Backspace.
    await page.keyboard.press('End');
    for (let i = 0; i < SAMPLES; i++) await page.keyboard.press('Backspace');
    const undone = await measure(page, 'Control+z');
    const results = [p95(`${execution} keystroke state`, typed.state), p95(`${execution} undo state`, undone.state), p95(`${execution} caret move state`, moved.state)];
    for (const r of results) expect(r).toBeLessThanOrEqual(REFRESH);
  });
}

// The renderer is the page's best: WebGPU where there is an adapter, WebGL 2 with no WebGPU, CPU with neither. The
// engine runs on the main thread here, so the page's patches choose it.
for (const renderer of ['webgpu', 'webgl', 'cpu'] as const) {
  test(`a keystroke's frame is painted within a refresh at devicePixelRatio 2 (${renderer})`, async ({ page }) => {
    if (renderer === 'webgl') await page.addInitScript(() => Object.defineProperty(navigator, 'gpu', { value: undefined }));
    await open(page, { gpu: renderer !== 'cpu' });
    expect(await page.evaluate(() => devicePixelRatio)).toBe(2);
    await ready(page, 'main');
    const got = await backend(page);
    test.skip(got !== renderer, `no ${renderer} on this machine (the editor drew with ${got})`);
    const typed = await measure(page, 'a', true);
    p95(`${renderer} keystroke draw`, typed.draw);
    expect(p95(`${renderer} keystroke frame`, typed.frame)).toBeLessThanOrEqual(REFRESH);
  });
}
