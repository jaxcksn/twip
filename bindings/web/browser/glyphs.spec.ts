// The text-glyphs render fixture's two pages (sizes 8 to 72 pt; colours and a turned cell), drawn in the browser, against the render gate's committed snapshot
// (crates/twip/tests/render) at the gate's tolerance: the wasm CPU path, and WebGPU where there is a hardware GPU.
import { readFileSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';
import { fixture, softwareGpu, until } from './helpers.ts';

const crates = new URL('../../../crates/', import.meta.url);
const bytes = (path: string) => [...readFileSync(new URL(path, crates))];
const snapshots = [1, 2].map((p) => readFileSync(new URL(`twip/tests/render/render__text-glyphs__p${p}.png`, crates)));

/** A pixel fails when a channel differs by more than this; a page when more than one pixel in `PER_FAILED` fails. */
const CHANNEL = 2;
const PER_FAILED = 5000;

/** The render gate's fonts (`bundled_fonts`): the stand-ins as the generic families and fallbacks, and JetBrains Mono. */
async function openWithGateFonts(page: Page, gpu: boolean): Promise<void> {
  page.on('pageerror', (e) => console.log(`page error: ${e.message}`));
  await page.route('**/blank.html', (route) =>
    route.fulfill({ contentType: 'text/html', body: '<!doctype html><meta charset="utf-8"><body style="margin:0">' }),
  );
  await page.goto('/blank.html');
  const stand = ['sans', 'serif', 'mono', 'cjk'].map((s) => bytes(`twip/tests/fixtures/editor/fonts/stand-in-${s}.ttf`));
  const mono = bytes('twip-graphics/tests/fonts/jetbrains-mono-500.ttf');
  await page.evaluate(async ({ gpu, stand, mono }) => {
    // The CPU path: no WebGPU adapter, and the engine on the main thread, where these patches are.
    if (!gpu) {
      Object.defineProperty(navigator, 'gpu', { value: undefined });
      Object.defineProperty(HTMLCanvasElement.prototype, 'transferControlToOffscreen', { value: undefined });
    }
    const twip = await import('/web/dist/index.js' as string);
    const fonts = twip.FontLibrary.default();
    const [sans, serif, monospace, cjk] = await Promise.all(stand.map(async (b: number[]) => (await fonts.add(new Uint8Array(b)))[0].family));
    await fonts.add(new Uint8Array(mono));
    fonts.setGenericDefaults({ sansSerif: sans, serif, monospace });
    fonts.setFallback(sans);
    fonts.setCharacterFallback([cjk]);
    Object.assign(window, { twip });
  }, { gpu, stand, mono });
}

/** A PNG's straight RGBA pixels, decoded by the browser. */
async function decode(page: Page, png: Buffer): Promise<{ width: number; height: number; px: Buffer }> {
  const { width, height, data } = await page.evaluate(async (b64) => {
    const bitmap = await createImageBitmap(await (await fetch(`data:image/png;base64,${b64}`)).blob(), { colorSpaceConversion: 'none', premultiplyAlpha: 'none' });
    const c = new OffscreenCanvas(bitmap.width, bitmap.height).getContext('2d')!;
    c.drawImage(bitmap, 0, 0);
    const bytes = c.getImageData(0, 0, bitmap.width, bitmap.height).data;
    let binary = '';
    for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    return { width: bitmap.width, height: bitmap.height, data: btoa(binary) };
  }, png.toString('base64'));
  return { width, height, px: Buffer.from(data, 'base64') };
}

type Image = { width: number; height: number; px: Buffer };

/** Whether pixel (x, y) has ink: a red channel under 64. */
const ink = (img: Image, x: number, y: number) => img.px[(y * img.width + x) * 4]! < 64;

/** The first row with ink from row `top` down, and the first column with ink in that row and the `height - 1` below. */
function inkCorner(img: Image, top = 0, height = img.height): [number, number] {
  let y0 = top;
  while (y0 < img.height && ![...Array(img.width).keys()].some((x) => ink(img, x, y0))) y0++;
  let x0 = img.width;
  for (let y = y0; y < Math.min(y0 + height, img.height); y++) {
    for (let x = 0; x < x0; x++) if (ink(img, x, y)) x0 = x;
  }
  return [x0, y0];
}

/** How many of the snapshot's pixels fail against the drawing, the snapshot placed by its ink's corner from row `top`
 * down, and where the page ends in the drawing. */
function failing(want: Image, got: Image, top: number): [number, number] {
  const [wx, wy] = inkCorner(want);
  const [gx, gy] = inkCorner(got, top, want.height - wy);
  const [dx, dy] = [gx - wx, gy - wy];
  expect(dx >= 0 && dy >= 0 && dx + want.width <= got.width && dy + want.height <= got.height, `the page at (${dx}, ${dy}) is on screen`).toBe(true);
  let failed = 0;
  for (let y = 0; y < want.height; y++) {
    for (let x = 0; x < want.width; x++) {
      const [a, b] = [(y * want.width + x) * 4, ((y + dy) * got.width + x + dx) * 4];
      if ([0, 1, 2].some((c) => Math.abs(want.px[a + c]! - got.px[b + c]!) > CHANNEL)) failed++;
    }
  }
  return [failed, dy + want.height];
}

async function drawnLikeTheSnapshot(page: Page, gpu: boolean): Promise<void> {
  // Both pages on screen. The main-thread transport keeps the context on the DOM canvas, which the probe below reads.
  await page.setViewportSize({ width: 1000, height: 2300 });
  await openWithGateFonts(page, gpu);
  const docx = fixture('render/text-glyphs.docx');
  await page.evaluate(async (docx) => {
    const container = Object.assign(document.createElement('div'), { id: 'editor' });
    container.style.cssText = 'width:1000px;height:2300px;';
    document.body.append(container);
    const w = window as unknown as Record<string, any>;
    w.editor = await w.twip.Editor.mount(container, { document: new Uint8Array(docx), background: '#FFFFFF', execution: 'main' });
  }, docx);
  await until(page, 'state.pages.complete');
  if (gpu) {
    // A canvas gives only the kind of context it already has: without a WebGPU device the editor drew on the CPU.
    const got = await page.evaluate(() => {
      const canvas = document.querySelector('#editor canvas') as HTMLCanvasElement;
      return canvas.getContext('webgpu') ? 'webgpu' : 'cpu';
    });
    test.skip(got !== 'webgpu', `no WebGPU on this machine (the editor drew with ${got})`);
  }
  const wants = await Promise.all(snapshots.map((png) => decode(page, png)));
  // Each page's failing pixels past the allowance. The frame after layout completes may still be on its way.
  const excess = async () => {
    const got = await decode(page, await page.locator('#editor canvas').first().screenshot());
    let top = 0;
    return wants.map((want) => {
      const [failed, bottom] = failing(want, got, top);
      top = bottom;
      return Math.max(0, failed - Math.floor((want.width * want.height) / PER_FAILED));
    });
  };
  await expect.poll(excess, { timeout: 10_000 }).toEqual([0, 0]);
}

test('text glyphs draw on the CPU as the render snapshot does', async ({ page }) => {
  await drawnLikeTheSnapshot(page, false);
});

test('text glyphs draw on WebGPU as the render snapshot does', { tag: '@webgpu' }, async ({ page }) => {
  test.skip(softwareGpu, 'a software GPU paints blank');
  await drawnLikeTheSnapshot(page, true);
});
