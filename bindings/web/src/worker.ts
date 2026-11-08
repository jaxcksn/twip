// The page's engine worker entry (`@twip/web/worker.js`). The SDK starts it itself; applications never load it.
import './worker/entry.js';
import { configure } from './transport/module.js';

// The page's `assets` (configure()), in the query it started this worker with: the engine loads from there, not from
// this file's own URL. Applied before the first request, which loads the engine (worker/entry.ts).
const assets = new URL(import.meta.url).searchParams.get('assets');
if (assets) configure({ assets });
