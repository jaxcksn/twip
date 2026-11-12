// Serves target/twip-graphics-web (crates/twip-graphics-web/build.mjs builds it) for gpu.spec.mjs, on PORT.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { dirname, extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(process.env.CARGO_TARGET_DIR ?? join(here, '..', '..', '..', 'target'), 'twip-graphics-web');
const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm' };
createServer(async (req, res) => {
  const path = normalize(new URL(req.url, 'http://localhost').pathname).replace(/^([/\\])+/, '');
  try {
    const body = await readFile(join(root, path || 'index.html'));
    res.writeHead(200, { 'content-type': types[extname(path || '.html')] ?? 'application/octet-stream' });
    res.end(body);
  } catch {
    res.writeHead(404);
    res.end();
  }
}).listen(Number(process.env.PORT ?? 8791), '127.0.0.1');
