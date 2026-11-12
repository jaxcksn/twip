import { defineConfig, devices } from '@playwright/test';

// twip-graphics' GPU comparisons on WebGPU (graphics/gpu.spec.mjs), apart from the editor's browser tests: the
// installed Edge, headless, with WebGPU on.
const port = Number(process.env.TWIP_GRAPHICS_PORT ?? 8791);
export default defineConfig({
  testDir: 'graphics',
  testMatch: '*.spec.mjs',
  outputDir: '../../target/playwright-graphics',
  timeout: 180_000,
  workers: 1,
  reporter: 'list',
  use: {
    ...devices['Desktop Edge'],
    channel: process.env.PW_CHANNEL ?? 'msedge',
    headless: true,
    baseURL: `http://127.0.0.1:${port}/`,
    launchOptions: { args: ['--enable-unsafe-webgpu'] },
  },
  webServer: { command: 'node graphics/serve.mjs', url: `http://127.0.0.1:${port}/index.html`, reuseExistingServer: true, env: { PORT: String(port) } },
});
