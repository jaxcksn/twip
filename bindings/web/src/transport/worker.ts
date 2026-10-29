// The engine in the page's engine worker (src/worker/entry.ts). One worker per page hosts every editor's session, so
// fonts, dictionaries and the internal clipboard are shared. Calls are posted in submission order and the worker runs
// them through one queue (a DirectTransport there), so the ordering is the direct transport's. Each operation's
// messages arrive before its reply and are dispatched in arrival order, so a state message reaches the session's
// cache before any event of the same drain.
import { EditorError, unsupported } from '../errors.js';
import type { FaceInfo } from '../fonts/library.js';
import { live } from '../host/dom.js';
import type { Warning } from '../types.js';
import { type Backend, type FrameInfo, type Input, type Message, Method, type Transport, toError } from './protocol.js';

/** A call to the worker: a {@link Transport} method and its arguments (without the signal), or an abort. */
export interface Request {
  id: number;
  op: keyof Transport | 'abort';
  args: unknown[];
}

/** The worker's answer to a request. */
export type Reply =
  | { id: number; ok: unknown }
  | { id: number; error: { code: EditorError['code']; message: string; details?: unknown }; fatal: boolean };

let modules: boolean | undefined;

/** Whether this page can run the engine in a worker: module workers and `transferControlToOffscreen`. */
function workerSupported(): boolean {
  if (typeof Worker === 'undefined' || typeof HTMLCanvasElement === 'undefined') return false;
  if (typeof HTMLCanvasElement.prototype.transferControlToOffscreen !== 'function') return false;
  if (modules === undefined) {
    // A browser with module workers reads `type`; the URL doesn't parse, so no worker starts.
    modules = false;
    try {
      new Worker('http://[', {
        get type() {
          modules = true;
          return 'module' as const;
        },
      });
    } catch {
      // Expected: the URL.
    }
  }
  return modules;
}

/**
 * The mode `execution` asks for. `'auto'` is the worker where the page supports it, and the main thread otherwise;
 * `'worker'` where it doesn't is `unsupported`.
 */
export function chooseExecution(execution: 'auto' | 'main' | 'worker' = 'auto'): 'main' | 'worker' {
  if (execution === 'main') return 'main';
  if (workerSupported()) return 'worker';
  if (execution === 'worker') throw unsupported("execution: 'worker' on this browser");
  return 'main';
}

let shared: WorkerTransport | undefined;

/** The page's one worker transport. */
export function workerTransport(): WorkerTransport {
  return (shared ??= new WorkerTransport());
}

/** The {@link Transport} that posts every call to the page's engine worker. */
export class WorkerTransport implements Transport {
  #worker: Worker | undefined;
  #next = 1;
  readonly #calls = new Map<number, { resolve: (value: unknown) => void; reject: (error: EditorError) => void }>();
  readonly #listeners = new Map<number, Set<(message: Message) => void>>();
  /** Open sessions: the worker stops when the last one closes and no call is in flight. */
  readonly #sessions = new Set<number>();
  /** The font library's changes, replayed into a restarted worker so the page's fonts survive it. */
  readonly #library: Request[] = [];
  /** Set when the worker failed: it can't be trusted again. */
  #fault: EditorError | undefined;

  createSession(options: object): Promise<number> {
    return this.#request<number>('createSession', [options]).then((id) => (this.#sessions.add(id), id));
  }

  closeSession(session: number): Promise<void> {
    return this.#request<void>('closeSession', [session]).finally(() => {
      this.#listeners.delete(session);
      this.#sessions.delete(session);
      this.#stopWhenIdle();
    });
  }

  call(session: number, method: Method, request?: unknown): Promise<unknown> {
    return this.#request('call', [session, method, request], { library: session === 0 && method !== Method.FontFamilies });
  }

  open(session: number, bytes: Uint8Array, signal?: AbortSignal): Promise<Warning[]> {
    return this.#request('open', [session, bytes], { signal });
  }

  openPrepare(session: number, bytes: Uint8Array): Promise<number> {
    return this.#request('openPrepare', [session, bytes]);
  }

  openCommit(session: number, token: number): Promise<Warning[]> {
    return this.#request('openCommit', [session, token]);
  }

  openDiscard(session: number, token: number): Promise<void> {
    return this.#request('openDiscard', [session, token]);
  }

  export(session: number, options: { format: 'docx' | 'pdf' }, signal?: AbortSignal) {
    return this.#request<{ bytes: Uint8Array; revision: number; warnings: Warning[] }>('export', [session, options], { signal });
  }

  addFont(bytes: Uint8Array): Promise<FaceInfo[]> {
    return this.#request('addFont', [bytes], { library: true });
  }

  input(session: number, input: Input): Promise<number> {
    return this.#request('input', [session, input]);
  }

  /** Moves the canvas to the worker, which draws the session's frames into it from then on. */
  attachCanvas(session: number, canvas: HTMLCanvasElement): Promise<Backend> {
    const offscreen = canvas.transferControlToOffscreen();
    return this.#request('attachCanvas', [session, offscreen], { transfer: [offscreen] });
  }

  frame(session: number, width: number, height: number): Promise<FrameInfo | null> {
    return this.#request('frame', [session, width, height]);
  }

  listen(session: number, listener: (message: Message) => void): () => void {
    let set = this.#listeners.get(session);
    if (!set) this.#listeners.set(session, (set = new Set()));
    set.add(listener);
    return () => set.delete(listener);
  }

  #request<T>(op: Request['op'], args: unknown[], how: { signal?: AbortSignal | undefined; transfer?: Transferable[]; library?: boolean } = {}): Promise<T> {
    if (this.#fault) return Promise.reject(this.#fault);
    const { signal } = how;
    if (signal?.aborted) return Promise.reject(new EditorError('aborted', 'the operation was aborted'));
    let worker: Worker;
    try {
      worker = this.#start();
    } catch (cause) {
      return Promise.reject(this.#failed(cause));
    }
    const request: Request = { id: this.#next++, op, args };
    // Replayed with no reply to wait for (id 0).
    if (how.library) this.#library.push({ ...request, id: 0 });
    return new Promise<T>((resolve, reject) => {
      const abort = () => worker.postMessage({ id: request.id, op: 'abort', args: [] } satisfies Request);
      signal?.addEventListener('abort', abort, { once: true });
      this.#calls.set(request.id, {
        resolve: (value) => (signal?.removeEventListener('abort', abort), resolve(value as T)),
        reject: (error) => (signal?.removeEventListener('abort', abort), reject(error)),
      });
      worker.postMessage(request, { transfer: how.transfer ?? [] });
    });
  }

  #start(): Worker {
    if (this.#worker) return this.#worker;
    // The worker is beside this file's directory, in dist/. Written out in one expression, as bundlers recognise
    // it: they build the worker and its imports into the application.
    const worker = new Worker(new URL('../worker.js', import.meta.url), { type: 'module', name: 'twip-engine' });
    live.workers++;
    worker.onmessage = ({ data }: MessageEvent<Message | Reply>) => this.#receive(data);
    worker.onerror = (e) => {
      e.preventDefault();
      this.#failed(e.message || 'the engine worker failed to start');
    };
    for (const request of this.#library) worker.postMessage(request);
    return (this.#worker = worker);
  }

  #receive(data: Message | Reply): void {
    if ('kind' in data) {
      for (const listener of [...(this.#listeners.get(data.sessionId) ?? [])]) {
        try {
          listener(data);
        } catch (e) {
          globalThis.reportError?.(e);
        }
      }
      return;
    }
    const call = this.#calls.get(data.id);
    if (!call) return;
    this.#calls.delete(data.id);
    if ('error' in data) {
      const e = toError(data.error);
      call.reject(data.fatal ? new EditorError(e.code, e.message, { details: e.details, fatal: true }) : e);
    } else call.resolve(data.ok);
    // After the caller's reaction, which counts a session just created.
    queueMicrotask(() => this.#stopWhenIdle());
  }

  /** No editor left: the worker goes, and the next call starts a new one. */
  #stopWhenIdle(): void {
    if (this.#sessions.size || this.#calls.size || !this.#worker) return;
    this.#stop();
  }

  #stop(): void {
    this.#worker?.terminate();
    if (this.#worker) live.workers--;
    this.#worker = undefined;
  }

  /** The worker failed: every call in flight and every later one rejects, and the next editor gets a new worker. */
  #failed(cause: unknown): EditorError {
    this.#fault ??= new EditorError('internal', `the engine worker stopped: ${String(cause)}`, { fatal: true, cause });
    if (shared === this) shared = undefined;
    for (const call of this.#calls.values()) call.reject(this.#fault);
    this.#calls.clear();
    this.#stop();
    return this.#fault;
  }
}
