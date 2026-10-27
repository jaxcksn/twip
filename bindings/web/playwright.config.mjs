import { defineConfig, devices } from '@playwright/test';

// The installed Edge, locally and on CI's Ubuntu runners (no browser download). PW_CHANNEL picks another.
const port = 8790;
export default defineConfig({
  testDir: 'browser',
  testMatch: '*.spec.mjs',
  outputDir: '../../target/playwright',
  timeout: 60_000,
  workers: 1,
  reporter: 'list',
  use: {
    ...devices['Desktop Edge'],
    channel: process.env.PW_CHANNEL ?? 'msedge',
    headless: true,
    baseURL: `http://127.0.0.1:${port}/`,
    launchOptions: { args: ['--enable-unsafe-webgpu'] },
  },
  webServer: { command: `node browser/serve.mjs`, url: `http://127.0.0.1:${port}/hello.docx`, reuseExistingServer: true, env: { PORT: String(port) } },
});
