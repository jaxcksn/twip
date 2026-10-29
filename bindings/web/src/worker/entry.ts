// The page's engine worker: one per page hosts every editor's session in one module instance, so fonts, dictionaries
// and the internal clipboard are shared. It runs each posted call through a DirectTransport, whose queue keeps them in
// arrival order, and relays the sessions' { kind, sessionId, payload } messages ahead of each reply. A session's canvas
// moves here, and its frames are drawn here when the page's frame loop asks (host/frames.ts); the engine's timers run
// on this thread.
import { EditorError } from '../errors.js';
import { directTransport, type DirectTransport } from '../transport/direct.js';
import { fromError, type Message } from '../transport/protocol.js';
import type { Reply, Request } from '../transport/worker.js';

const engine = directTransport();
const signals = new Map<number, AbortController>();

/** Posts to the page, moving the buffer of exported bytes instead of copying it. */
function post(message: Message | Reply): void {
  const bytes = 'ok' in message ? (message.ok as { bytes?: unknown } | null)?.bytes : undefined;
  postMessage(message, { transfer: bytes instanceof Uint8Array && bytes.buffer instanceof ArrayBuffer ? [bytes.buffer] : [] });
}

/** Runs one request on the engine. */
function run(t: DirectTransport, { id, op, args }: Request): Promise<unknown> {
  switch (op) {
    case 'createSession':
      return t.createSession(args[0] as object).then((session) => {
        t.listen(session, post);
        return session;
      });
    case 'attachCanvas':
      // An OffscreenCanvas: the GPU and CPU paths take either kind of canvas.
      return t.attachCanvas(args[0] as number, args[1] as HTMLCanvasElement);
    case 'open':
    case 'export': {
      const controller = new AbortController();
      signals.set(id, controller);
      const call = t[op] as (...a: unknown[]) => Promise<unknown>;
      return call.call(t, ...args, controller.signal).finally(() => signals.delete(id));
    }
    default:
      return (t[op as keyof DirectTransport] as (...a: unknown[]) => Promise<unknown>).apply(t, args);
  }
}

// Session 0 is the engine's own: font requests.
engine.then((t) => t.listen(0, post), () => {});

onmessage = ({ data }: MessageEvent<Request>) => {
  if (data.op === 'abort') {
    signals.get(data.id)?.abort();
    return;
  }
  // Every request waits on the same promise, so they reach the engine's queue in arrival order. Id 0 is a replayed
  // library change: nothing waits for its reply.
  engine.then((t) => run(t, data)).then(
    (ok) => data.id && post({ id: data.id, ok }),
    (e: unknown) => {
      if (!data.id) return console.error(e);
      const error = e instanceof EditorError ? e : new EditorError('internal', String(e), { cause: e });
      post({ id: data.id, error: fromError(error), fatal: error.fatal });
    },
  );
};
