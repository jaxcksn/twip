// A static server for the browser test: bindings/ as the root (the web package's dist/ and hello.docx), plus /kit/,
// the test font kit (crates/twip/tests/fonts; the engine has no fonts of its own in the browser), and /twip/, the
// package's dist/ again, as an application copies it (assets.spec.ts). The next port serves the same, as another origin (a CDN),
// and every response allows any origin, as a CDN would.
import { createServer } from 'node:http';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { extname, join, normalize, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '../..');
const port = Number(process.env.PORT ?? 8790);
const kit = resolve(root, '../crates/twip/tests/fonts');
const types = { '.html': 'text/html', '.mjs': 'text/javascript', '.js': 'text/javascript', '.wasm': 'application/wasm', '.docx': 'application/octet-stream', '.ttf': 'font/ttf', '.json': 'application/json' };

const server = createServer((req, res) => {
  const url = new URL(req.url, 'http://localhost');
  url.pathname = url.pathname.replace(/^\/twip\//, '/web/dist/');
  const [base, path] = url.pathname.startsWith('/kit/') ? [kit, url.pathname.slice('/kit'.length)] : [root, url.pathname];
  const file = join(base, normalize(decodeURIComponent(path)));
  if (!file.startsWith(base) || !existsSync(file) || statSync(file).isDirectory()) {
    res.writeHead(404).end();
    return;
  }
  res.writeHead(200, { 'content-type': types[extname(file)] ?? 'application/octet-stream', 'access-control-allow-origin': '*' }).end(readFileSync(file));
});
// Idle connections stay open until the browser closes them: with Node's 5 s timeout, a request the browser sends on a
// connection the server is just closing fails, as a module import does when the test before it took about 5 s.
server.keepAliveTimeout = 0;
server.listen(port, '127.0.0.1', () => console.log(`serving ${root} on http://127.0.0.1:${port}/`));
const cdn = createServer(server.listeners('request')[0]);
cdn.keepAliveTimeout = 0;
cdn.listen(port + 1, '127.0.0.1');
