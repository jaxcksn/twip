// The editor's performance scenarios (specs/010-editor-performance/contracts/scenarios.md), in worker mode as the
// demo runs it: typing, a burst of keys inside one frame, a column border drag, scrolling and opening. Each writes
// its samples and percentiles to target/perf/<id>-<gpu>-<dpr>.json. Tagged @perf: `moon run web:perf` runs them alone,
// on a quiet machine with a GPU; `web:browser-test` leaves them out.
import { mkdirSync, writeFileSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { test, type Page } from '@playwright/test';
import { fixture, mount, open, softwareGpu } from './helpers.ts';

test.skip(softwareGpu, 'a software GPU');
// Each scenario runs hundreds of inputs (typing: 300 keys 100 ms apart).
test.setTimeout(240_000);

const REFRESH = 1000 / 120;
const OUT = new URL('../../../target/perf/', import.meta.url);
const COMMIT = (() => {
  try {
    return execSync('git rev-parse --short HEAD').toString().trim();
  } catch {
    return 'unknown';
  }
})();

type Result = { id: string; document: string; samples_ms: number[]; [extra: string]: unknown };

/** Writes a scenario's JSON with its p50, p95 and max, and prints a line of them. */
async function record(page: Page, r: Result): Promise<void> {
  const sorted = [...r.samples_ms].sort((a, b) => a - b);
  const at = (q: number) => sorted[Math.min(sorted.length - 1, Math.max(0, Math.ceil(q * sorted.length) - 1))] ?? NaN;
  const { gpu, dpr } = await page.evaluate(() => {
    const canvas = document.querySelector('canvas') as HTMLCanvasElement | null;
    // In worker mode the canvas was transferred: the worker's backend isn't readable here.
    const gpu = (navigator as any).gpu ? 'webgpu' : 'webgl';
    return { gpu: canvas ? gpu : 'none', dpr: devicePixelRatio };
  });
  const out = { ...r, gpu, dpr, commit: COMMIT, p50: at(0.5), p95: at(0.95), max: sorted.at(-1) ?? NaN };
  mkdirSync(OUT, { recursive: true });
  writeFileSync(new URL(`${r.id}-${gpu}-${dpr}.json`, OUT), JSON.stringify(out, null, 2));
  console.log(`${r.id} ${r.document}: p50 ${out.p50.toFixed(1)} ms, p95 ${out.p95.toFixed(1)} ms, max ${out.max.toFixed(1)} ms (frame ${REFRESH.toFixed(1)})`);
}

/** Times each input event (keydown, pointermove or wheel) to the end of the first frame drawn after it, and watches the
 * main thread: the longest gap between animation frames while the scenario runs. */
async function instrument(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const { sessionOf } = await import('/web/dist/editor.js' as string);
    const w = window as any;
    w.samples = [];
    const start = (e: Event) => {
      // `seen`: the first state after the event; `asked`: the first frame call after it; `frame`: that frame drawn.
      const sample: any = { at: e.timeStamp, seen: 0, asked: 0, frame: 0 };
      sample.painted = new Promise((r) => (sample.paint = r));
      w.sample = sample;
    };
    document.addEventListener('keydown', start, true);
    document.addEventListener('pointermove', start, true);
    document.addEventListener('wheel', start, true);
    w.editor.subscribe(() => {
      if (w.sample && !w.sample.seen) w.sample.seen = performance.now();
    });
    const transport = sessionOf(w.editor).transport;
    const frame = transport.frame.bind(transport);
    transport.frame = (...args: unknown[]) => {
      const sample = w.sample;
      const asked = performance.now();
      if (sample && !sample.asked && asked >= sample.at) sample.asked = asked;
      return frame(...args).then((drawn: unknown) => {
        if (drawn && sample && !sample.frame && asked >= sample.at) {
          sample.frame = performance.now();
          sample.paint();
        }
        return drawn;
      });
    };
    w.gap = 0;
    let last = performance.now();
    const tick = (t: number) => {
      w.gap = Math.max(w.gap, t - last);
      last = t;
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });
}

/** The time from the last input event to its painted frame (Infinity if none came within 2 s); its stages go to
 * `stages` when given: event to state, state (or event) to the frame call, the call to the frame drawn. */
function painted(page: Page, stages?: number[][]): Promise<number> {
  return page
    .evaluate(async () => {
      const sample = (window as any).sample;
      await Promise.race([sample.painted, new Promise((r) => setTimeout(r, 2000))]);
      const seen = sample.seen || sample.at;
      return { total: sample.frame ? sample.frame - sample.at : Infinity, stages: [seen - sample.at, (sample.asked || seen) - seen, sample.frame ? sample.frame - (sample.asked || seen) : Infinity] };
    })
    .then((r) => {
      stages?.push(r.stages);
      return r.total;
    });
}

/** Each stage's median, from `painted`'s stage samples. */
function medians(stages: number[][]): { to_state: number; to_frame_call: number; drawing: number } {
  const median = (k: number) => [...stages.map((s) => s[k]!)].sort((a, b) => a - b)[Math.floor(stages.length / 2)] ?? NaN;
  return { to_state: median(0), to_frame_call: median(1), drawing: median(2) };
}

/** Resets the longest frame gap, after setup. */
const resetGap = (page: Page) => page.evaluate(() => ((window as any).gap = 0));
const gap = (page: Page): Promise<number> => page.evaluate(() => (window as any).gap);

/** An editor on `name` in worker mode, filling a 1000 x 800 viewport, laid out to the end. */
async function ready(page: Page, name: string): Promise<void> {
  await open(page);
  await mount(page, { docx: fixture(name), width: 1000, height: 800, mount: { execution: 'worker' } });
  await page.evaluate(async () => {
    const editor = (window as any).editor;
    for (let i = 0; i < 600 && !editor.getState().pages.complete; i++) await new Promise((r) => setTimeout(r, 50));
  });
  await instrument(page);
}

for (const dpr of [1, 2]) {
  test.describe(`dpr ${dpr}`, () => {
    test.use({ deviceScaleFactor: dpr });

    for (const name of ['docx-feature-showcase.docx', 'editor/FX-LONG-50.docx']) {
      test(`@perf typing in ${name}`, async ({ page }) => {
        await ready(page, name);
        await page.locator('#editor canvas').click({ position: { x: 400, y: 300 } });
        await page.keyboard.press('End');
        await resetGap(page);
        const samples: number[] = [];
        const stages: number[][] = [];
        for (let i = 0; i < 300; i++) {
          await page.keyboard.press(i % 10 === 9 ? 'Backspace' : 'a');
          samples.push(await painted(page, stages));
          await page.waitForTimeout(100);
        }
        const frame_stats = await page.evaluate(() => (window as any).editor.view.frameStats());
        console.log(`typing ${name} stages (median ms):`, JSON.stringify(medians(stages)));
        await record(page, { id: 'typing', document: name, samples_ms: samples, stages_ms: medians(stages), longest_gap_ms: await gap(page), frame_stats });
      });
    }

    test('@perf a burst of keys inside one frame shows in the next', async ({ page }) => {
      await ready(page, 'docx-feature-showcase.docx');
      await page.locator('#editor canvas').click({ position: { x: 400, y: 300 } });
      await resetGap(page);
      const samples: number[] = [];
      for (let i = 0; i < 20; i++) {
        // Five keys with no delay between them, most inside one frame: the frame after the last one shows them.
        await page.keyboard.type('abcde');
        samples.push(await painted(page));
        await page.waitForTimeout(100);
      }
      await record(page, { id: 'burst', document: 'docx-feature-showcase.docx', samples_ms: samples, longest_gap_ms: await gap(page) });
    });

    for (const name of ['editor/FX-TBL-LARGE.docx', 'editor/FX-TBL-LARGE-NESTED.docx']) {
      test(`@perf a column border drag in ${name}`, async ({ page }) => {
        await ready(page, name);
        // The border left of the second column: hover across the first column's right edge until the pointer shows
        // the resize cursor.
        const pg = await page.evaluate(() => (window as any).editor.view.visiblePages()[0].rect);
        const y = pg.top + 150;
        let x = 0;
        for (let at = pg.left + 150; at < pg.left + 400 && !x; at += 1) {
          await page.mouse.move(at, y);
          if ((await page.evaluate(() => (document.querySelector('#editor canvas')?.parentElement as HTMLElement).style.cursor)) === 'col-resize') x = at;
        }
        test.skip(!x, 'no column border found');
        await page.mouse.down();
        await resetGap(page);
        const moves: number[] = [];
        // Two seconds of moves at the display rate, back and forth.
        for (let i = 0; i < 240; i++) {
          await page.mouse.move(x + 10 + 30 * Math.sin(i / 20), y);
          moves.push(await painted(page));
        }
        const longest = await gap(page);
        await page.mouse.up();
        // ponytail: the release's frame is timed from the last move's event; a pointerup listener would time it alone.
        const release = await painted(page);
        await record(page, { id: 'drag-column', document: name, samples_ms: moves, release_ms: release, longest_gap_ms: longest });
      });
    }

    test('@perf scrolling from the first page to the last', async ({ page }) => {
      await ready(page, 'editor/FX-LONG-50.docx');
      await resetGap(page);
      const samples: number[] = [];
      // Down past the 50 pages (400 px a step), then back up: every step moves, so every step draws.
      await page.mouse.move(500, 400);
      for (let i = 0; i < 300; i++) {
        await page.mouse.wheel(0, i < 150 ? 400 : -400);
        samples.push(await painted(page));
      }
      await record(page, { id: 'scroll', document: 'editor/FX-LONG-50.docx', samples_ms: samples, longest_gap_ms: await gap(page) });
    });

    for (const name of ['docx-feature-showcase.docx', 'editor/FX-LONG-100.docx']) {
      // Opening is timed from the document's bytes: one editor is mounted (its surface set up once, as a page does), and
      // each sample opens the document into it, to the first frame drawn after the open.
      test(`@perf opening ${name}`, async ({ page }) => {
        await open(page);
        await mount(page, { width: 1000, height: 800, mount: { execution: 'worker' } });
        const samples = await page.evaluate(async ({ docx }) => {
          const editor = (window as any).editor;
          const out: number[] = [];
          for (let i = 0; i < 8; i++) {
            const before = (await editor.view.frameStats()).frames;
            const t = performance.now();
            await editor.open(new Uint8Array(docx));
            while ((await editor.view.frameStats()).frames <= before) await new Promise((r) => requestAnimationFrame(r));
            out.push(performance.now() - t);
          }
          return out;
        }, { docx: fixture(name) });
        await record(page, { id: 'open', document: name, samples_ms: samples });
      });
    }

    test('@perf start-up, cold and warm', async ({ page }) => {
      // Cold: from navigation to an empty editor mounted; warm: a second one in the same page.
      await open(page);
      const [cold, warm] = await page.evaluate(async () => {
        const w = window as any;
        const mountOne = async (id: string) => {
          const container = Object.assign(document.createElement('div'), { id });
          container.style.cssText = 'width:400px;height:300px;';
          document.body.append(container);
          return w.twip.Editor.mount(container, { execution: 'worker' });
        };
        await mountOne('cold');
        const cold = performance.now();
        const t = performance.now();
        await mountOne('warm');
        return [cold, performance.now() - t];
      });
      await record(page, { id: 'startup', document: '-', samples_ms: [cold], warm_ms: warm });
    });
  });
}
