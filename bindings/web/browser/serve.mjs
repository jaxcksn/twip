// A static server for the browser test: bindings/ as the root (the web package's dist/ and hello.docx), plus
// /font.ttf, a font from this machine (the engine has no fonts of its own in the browser).
import { createServer } from 'node:http';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { extname, join, normalize, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '../..');
const port = Number(process.env.PORT ?? 8790);
const fonts = ['C:/Windows/Fonts/calibri.ttf', 'C:/Windows/Fonts/arial.ttf', '/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf'];
const types = { '.html': 'text/html', '.mjs': 'text/javascript', '.js': 'text/javascript', '.wasm': 'application/wasm', '.docx': 'application/octet-stream', '.ttf': 'font/ttf' };

createServer((req, res) => {
  const url = new URL(req.url, 'http://localhost');
  const file = url.pathname === '/font.ttf' ? fonts.find(existsSync) : join(root, normalize(decodeURIComponent(url.pathname)));
  if (!file || !file.startsWith(url.pathname === '/font.ttf' ? '' : root) || !existsSync(file) || statSync(file).isDirectory()) {
    res.writeHead(404).end();
    return;
  }
  res.writeHead(200, { 'content-type': types[extname(file)] ?? 'application/octet-stream' }).end(readFileSync(file));
}).listen(port, '127.0.0.1', () => console.log(`serving ${root} on http://127.0.0.1:${port}/`));
