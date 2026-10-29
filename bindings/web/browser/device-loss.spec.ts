// GPU device loss: a lost WebGL context (WEBGL_lose_context, then restored) or a destroyed WebGPU device is replaced
// with no host code, on the main thread and in the worker, and an edit's frame follows within a second. Past 3 lost
// devices, the canvas shows CPU pixels.
import { expect, test, type Page } from '@playwright/test';
import { hello, mount, open, pixels } from './helpers.ts';

/**
 * Runs in the page, and in the engine worker ahead of its script: keeps each WebGPU device and WebGL 2 context made,
 * counts CPU pixels written into WebGPU textures, and `twipLose()` loses the latest (a WebGL context is restored 100 ms
 * later). `webgl` hides WebGPU.
 */
function instrument(webgl: boolean): void {
  const g = globalThis as any;
  if (webgl) Object.defineProperty(navigator, 'gpu', { value: undefined });
  const made = { devices: [] as any[], gl: [] as any[], writes: 0 };
  const queue = g.GPUQueue?.prototype;
  if (queue) {
    const write = queue.writeTexture;
    queue.writeTexture = function (this: unknown, ...args: unknown[]) {
      made.writes++;
      return write.apply(this, args);
    };
  }
  const adapter = g.GPUAdapter?.prototype;
  if (adapter) {
    const request = adapter.requestDevice;
    adapter.requestDevice = async function (this: unknown, ...args: unknown[]) {
      const device = await request.apply(this, args);
      made.devices.push(device);
      return device;
    };
  }
  for (const kind of [g.HTMLCanvasElement, g.OffscreenCanvas]) {
    if (!kind) continue;
    const get = kind.prototype.getContext;
    kind.prototype.getContext = function (this: unknown, type: string, ...rest: unknown[]) {
      const context = get.call(this, type, ...rest);
      if (type === 'webgl2' && context && !made.gl.includes(context)) made.gl.push(context);
      return context;
    };
  }
  g.twipMade = () => ({ devices: made.devices.length, gl: made.gl.length, lost: made.gl.at(-1)?.isContextLost() ?? false, writes: made.writes });
  g.twipLose = () => {
    const gl = made.gl.at(-1);
    if (gl) {
      const lose = gl.getExtension('WEBGL_lose_context');
      lose.loseContext();
      setTimeout(() => lose.restoreContext(), 100);
    } else made.devices.at(-1)?.destroy();
  };
}

/** Where the engine draws: the page, or the engine worker. */
function engineScope(page: Page, execution: 'main' | 'worker') {
  if (execution === 'main') return page;
  const worker = page.workers().find((w) => w.url().includes('/dist/worker.js'));
  if (!worker) throw new Error('no engine worker');
  return worker;
}

/** How much of `after` differs from `before`: the share of pixels off by more than antialiasing. */
function difference(before: Awaited<ReturnType<typeof pixels>>, after: Awaited<ReturnType<typeof pixels>>): number {
  let off = 0;
  for (let y = 0; y < before.height; y++) {
    for (let x = 0; x < before.width; x++) {
      const [a, b] = [before.at(x, y), after.at(x, y)];
      const channel = (shift: number) => Math.abs(((a >> shift) & 0xff) - ((b >> shift) & 0xff));
      if (Math.max(channel(16), channel(8), channel(0)) > 24) off++;
    }
  }
  return off / (before.width * before.height);
}

for (const backend of ['webgl', 'webgpu'] as const) {
  for (const execution of ['main', 'worker'] as const) {
    test(`a lost ${backend} ${backend === 'webgl' ? 'context' : 'device'} draws again within a second (${execution})`, async ({ page }) => {
      const webgl = backend === 'webgl';
      await page.addInitScript(instrument, webgl);
      await page.route('**/web/dist/worker.js', async (route) => {
        const response = await route.fetch();
        await route.fulfill({ response, body: `(${instrument.toString()})(${webgl});\n${await response.text()}` });
      });
      await open(page);
      await mount(page, { docx: hello, mount: { execution } });
      const scope = engineScope(page, execution);
      const made = await scope.evaluate(() => (globalThis as any).twipMade());
      test.skip(webgl ? made.gl === 0 : made.devices === 0, `no ${backend} on this machine`);
      const canvas = page.locator('#editor canvas');
      const before = await pixels(page, canvas);

      await scope.evaluate(() => (globalThis as any).twipLose());
      const lost = Date.now();
      if (webgl) expect((await scope.evaluate(() => (globalThis as any).twipMade())).lost, 'the context is lost').toBe(true);
      // An edit while the GPU is gone (a lost WebGPU canvas keeps showing its last frame): its frame shows within a
      // second of the loss (of the restore, for WebGL, 100 ms after it).
      await page.evaluate(() => (window as any).editor.commands.insertText('WWWWWWWWWWWW'));
      const due = lost + (webgl ? 1100 : 1000);
      let drawn = Infinity;
      while (Date.now() < due + 3000) {
        // The text, not a blank canvas.
        const changed = difference(before, await pixels(page, canvas));
        if (changed > 0.0002 && changed < 0.02) {
          drawn = Date.now();
          break;
        }
      }
      expect(drawn - lost, 'the frame after the loss').toBeLessThanOrEqual(due - lost);
      const now = await scope.evaluate(() => (globalThis as any).twipMade());
      if (webgl) expect(now.lost, 'the context is restored').toBe(false);
      else expect(now.devices, 'a new device').toBe(2);
      // Undone, the frame is the one before the loss.
      await page.evaluate(() => (window as any).editor.commands.undo());
      await expect.poll(async () => difference(before, await pixels(page, canvas))).toBeLessThan(0.0002);
    });
  }
}

for (const execution of ['main', 'worker'] as const) {
  test(`past 3 lost WebGPU devices the canvas shows CPU pixels (${execution})`, async ({ page }) => {
    await page.addInitScript(instrument, false);
    await page.route('**/web/dist/worker.js', async (route) => {
      const response = await route.fetch();
      await route.fulfill({ response, body: `(${instrument.toString()})(false);
${await response.text()}` });
    });
    await open(page);
    await mount(page, { docx: hello, mount: { execution } });
    const scope = engineScope(page, execution);
    test.skip((await scope.evaluate(() => (globalThis as any).twipMade())).devices === 0, 'no webgpu on this machine');
    const canvas = page.locator('#editor canvas');
    const before = await pixels(page, canvas);
    for (let devices = 2; devices <= 5; devices++) {
      if (devices === 5) expect((await scope.evaluate(() => (globalThis as any).twipMade())).writes, 'GPU frames for 3 losses').toBe(0);
      await scope.evaluate(() => (globalThis as any).twipLose());
      await expect.poll(async () => (await scope.evaluate(() => (globalThis as any).twipMade())).devices).toBe(devices);
    }
    await page.evaluate(() => (window as any).editor.commands.insertText('WWWWWWWWWWWW'));
    await expect.poll(async () => {
      const changed = difference(before, await pixels(page, canvas));
      return changed > 0.0002 && changed < 0.02;
    }, { message: 'the edit shows' }).toBe(true);
    expect((await scope.evaluate(() => (globalThis as any).twipMade())).writes, 'CPU frames').toBeGreaterThan(0);
  });
}
