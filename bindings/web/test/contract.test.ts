// The shared contract scripts (tests/contract/*.json, format in its README) against a headless editor with the
// engine on this thread. Every surface runs the same scripts; a failure names the runner, the script and the step.
// Scripts listed in tests/contract/pending.txt are skipped while they fail, and fail the run once they pass.
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { test } from 'node:test';
import { isMainThread, parentPort, Worker, workerData } from 'node:worker_threads';
import { EditorError, FontLibrary, HeadlessEditor, type Commands, type Op, type TwipEvents } from '../dist/index.js';
import { fromOp } from '../dist/commands.js';
import { encodeEvent } from '../dist/transport/protocol.js';

const RUNNER = 'web-main';
const root = new URL('../../../', import.meta.url);
const scripts = new URL('tests/contract/', root);
const fixtures = new URL('crates/twip/tests/fixtures/', root);
const engine = new URL('../dist/twip-wasm.js', import.meta.url);

type Step = { op: string } & Record<string, unknown>;
interface Script {
  name?: string;
  fixture?: string;
  fonts?: 'system' | 'empty';
  steps: Step[];
}

/** Every event type, so the runner records them all. */
const eventTypes: Record<keyof TwipEvents, true> = {
  documentChanged: true, selectionChanged: true, linkActivated: true, placeholderActivated: true, contextMenu: true,
  dialogRequested: true, hostCommand: true, shortcut: true, gesture: true, wordAdded: true, fontsChanged: true,
  elementActivated: true, elementsChanged: true, fieldsUpdateRequested: true, revisionsChanged: true,
  commentsChanged: true, commentAnchors: true, balloonsLaidOut: true, rulerChanged: true, tooltip: true,
  announcement: true, warning: true, error: true,
};

/** A step that sends input: a headless editor takes none, so scripts with one don't run here. */
const inputSteps = new Set(['key', 'text', 'click']);

/** Whether a step sends input, itself or in the steps it runs. */
const sendsInput = (s: Step): boolean =>
  inputSteps.has(s.op) || (s.op === 'concurrent' && (s.steps as Step[]).some(sendsInput)) || (s.op === 'repeat' && sendsInput(s.step as Step));

/** A step's failure, already naming the runner, the script and the step. */
class Failure extends Error {}

const isObject = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);

/** The JSON type name of a value. */
function typeName(v: unknown): string {
  if (v === null || v === undefined) return 'null';
  if (Array.isArray(v)) return 'array';
  return typeof v === 'object' ? 'object' : typeof v;
}

/** An array's elements and those of their nested `items` arrays (submenus), for `$has` and `$saved` paths. */
function items(v: unknown): unknown[] {
  if (!Array.isArray(v)) return [];
  return v.flatMap((e) => [e, ...(isObject(e) ? items(e.items) : [])]);
}

/** Exact equality, with `undefined` as null. */
function equal(a: unknown, b: unknown): boolean {
  if (Array.isArray(a)) return Array.isArray(b) && a.length === b.length && a.every((x, i) => equal(x, b[i]));
  if (isObject(a)) {
    if (!isObject(b)) return false;
    const keys = new Set([...Object.keys(a), ...Object.keys(b)].filter((k) => a[k] !== undefined || b[k] !== undefined));
    return [...keys].every((k) => equal(a[k], b[k]));
  }
  return (a ?? null) === (b ?? null);
}

/** Whether each of `wants` matches a different element of `got`, in any order. */
function assign(wants: unknown[], got: unknown[], used: boolean[]): boolean {
  if (wants.length === 0) return true;
  const [first, ...rest] = wants;
  for (const [i, g] of got.entries()) {
    if (used[i] || mismatch(g, first, '') !== null) continue;
    used[i] = true;
    if (assign(rest, got, used)) return true;
    used[i] = false;
  }
  return false;
}

/** Why `actual` doesn't partially match `expected` (README "Matching"), or null when it does. */
function mismatch(actual: unknown, expected: unknown, path: string): string | null {
  const show = (v: unknown) => JSON.stringify(v) ?? 'undefined';
  if (isObject(expected) && Object.keys(expected).length === 1) {
    if ('$exact' in expected) return equal(actual, expected.$exact) ? null : `${path}: expected exactly ${show(expected.$exact)}, got ${show(actual)}`;
    if ('$type' in expected) return typeName(actual) === expected.$type ? null : `${path}: expected a ${String(expected.$type)}, got ${show(actual)}`;
    if ('$has' in expected) {
      const all = items(actual);
      const has = expected.$has as unknown[];
      return Array.isArray(actual) && assign(has, all, all.map(() => false)) ? null : `${path}: ${show(actual)} doesn't have each of ${show(has)}`;
    }
  }
  if (Array.isArray(expected)) {
    if (!Array.isArray(actual)) return `${path}: expected an array, got ${show(actual)}`;
    if (actual.length < expected.length) return `${path}: expected at least ${expected.length} item(s), got ${actual.length}: ${show(actual)}`;
    for (const [i, e] of expected.entries()) {
      const m = mismatch(actual[i], e, `${path}[${i}]`);
      if (m) return m;
    }
    return null;
  }
  if (isObject(expected)) {
    if (!isObject(actual)) return `${path}: expected an object, got ${show(actual)}`;
    for (const [k, e] of Object.entries(expected)) {
      const m = mismatch((actual as Record<string, unknown>)[k], e, `${path}.${k}`);
      if (m) return m;
    }
    return null;
  }
  return Object.is(actual ?? null, expected) ? null : `${path}: expected ${show(expected)}, got ${show(actual)}`;
}

/** `wants`, in order, against a subsequence of `events`. */
function subsequence(events: unknown[], wants: unknown[]): string | null {
  let next = 0;
  for (const [i, want] of wants.entries()) {
    const at = events.slice(next).findIndex((e) => mismatch(e, want, '') === null);
    if (at < 0) return `events[${i}]: no later event matches ${JSON.stringify(want)}; the events were ${JSON.stringify(events)}`;
    next += at + 1;
  }
  return null;
}

async function run(file: string, script: Script): Promise<void> {
  const editor = await HeadlessEditor.create();
  const events: unknown[] = [];
  for (const type of Object.keys(eventTypes) as (keyof TwipEvents)[]) {
    editor.on(type, (event) => events.push(encodeEvent(type, event)));
  }
  // Every runner checks the state at each event: a listener already reads the state at the event's revision.
  let inconsistent: string | undefined;
  editor.on('documentChanged', (record) => {
    const now = editor.getState().revision;
    if (now !== record.revision) inconsistent ??= `documentChanged for revision ${record.revision}, but getState() is at ${now}`;
  });
  const background: Promise<unknown>[] = [];
  const saved = new Map<string, unknown>();
  let abort: 'early' | 'late' | undefined;
  let exported: Uint8Array | undefined;
  let failed: { error: EditorError; at: string } | undefined;

  const name = file.replace(/\.json$/, '');
  const fail = (at: string, why: string): never => {
    throw new Failure(`${RUNNER}: ${name}, ${at}: ${why}`);
  };
  const consistent = (at: string) => {
    if (inconsistent) fail(at, inconsistent);
  };
  const command = (op: Op) => {
    const [id, arg] = fromOp(op);
    return (editor.commands[id] as (arg?: unknown) => Promise<unknown>)(arg);
  };
  const fixture = (name: string) => new Uint8Array(readFileSync(new URL(name, fixtures)));

  /** `v` with every `$file` replaced by its bytes and every `$saved` by the saved value it names. */
  const resolve = (v: unknown, at: string): unknown => {
    if (Array.isArray(v)) return v.map((x) => resolve(x, at));
    if (!isObject(v)) return v;
    if ('$file' in v) return fixture(v.$file as string);
    if ('$saved' in v) {
      const name = v.$saved as string;
      if (!saved.has(name)) fail(at, `nothing saved as ${name}`);
      let value = saved.get(name);
      for (const step of (v.path as unknown[] | undefined) ?? []) {
        const next: unknown =
          typeof step === 'string' ? (value as Record<string, unknown> | null)?.[step]
          : typeof step === 'number' ? (value as unknown[] | null)?.[step]
          : items(value).find((e) => mismatch(e, step, '') === null);
        if (next === undefined) fail(at, `$saved ${name}: no ${JSON.stringify(step)} in ${JSON.stringify(value)}`);
        value = next;
      }
      return value;
    }
    return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, resolve(x, at)]));
  };

  /** The signal for the next operation: aborted before it runs, or only after it settles (`late`). */
  const signalFor = <T>(use: (signal: AbortSignal | undefined) => Promise<T>): Promise<T> => {
    const how = abort;
    abort = undefined;
    if (how === 'early') return use(AbortSignal.abort());
    if (how !== 'late') return use(undefined);
    const late = new AbortController();
    return use(late.signal).finally(() => late.abort());
  };

  /** Calls a member by its TypeScript path: `find.next`, `catalog`, `fonts.substitute`, `@saved.member`. */
  const call = async (method: string, args: unknown[], at: string): Promise<unknown> => {
    const path = method.split('.');
    const first = path.shift()!;
    let target: unknown =
      first.startsWith('@') ? saved.get(first.slice(1))
      : first === 'fonts' ? FontLibrary.default()
      : (editor as unknown as Record<string, unknown>)[first];
    let owner: unknown = editor;
    for (const key of path) {
      owner = target;
      target = (target as Record<string, unknown> | undefined)?.[key];
    }
    if (target === undefined) fail(at, `no member ${method}`);
    return typeof target === 'function' ? await (target as (...a: unknown[]) => unknown).apply(owner, args) : target;
  };

  /** Starts an operation step and returns its promise, submitting any engine call before it returns. */
  const start = (step: Step, at: string): Promise<unknown> => {
    switch (step.op) {
      case 'open':
        if (!script.fixture) fail(at, 'the script has no fixture');
        return signalFor((signal) => editor.open(fixture(script.fixture!), { signal })).then((result) => {
          const m = step.match === undefined ? null : mismatch(result, step.match, 'open');
          if (m) fail(at, m);
        });
      case 'openBytes':
        return signalFor((signal) => editor.open(Buffer.from(step.base64 as string, 'base64'), { signal }));
      case 'reopen':
        if (!exported) fail(at, 'reopen before any docx export');
        return signalFor((signal) => editor.open(exported!, { signal }));
      case 'export':
        return signalFor((signal) => editor.export({ format: step.format as 'docx' | 'pdf', signal })).then((result) => {
          const now = editor.getState().revision;
          if (result.revision !== now) fail(at, `the export reports revision ${result.revision}, the state is at ${now}`);
          const head = String.fromCharCode(...result.bytes.subarray(0, 4));
          const m = step.match === undefined ? null : mismatch({ revision: result.revision, warnings: result.warnings, head }, step.match, 'export');
          if (m) fail(at, m);
          if (step.format === 'docx') exported = result.bytes;
        });
      case 'command':
        return command(step.command as Op);
      case 'batch':
        return editor.batch(step.commands as Op[], step.label === undefined ? undefined : { label: step.label as string });
      case 'call':
        return call(step.method as string, (step.args as unknown[] | undefined) ?? [], at).then((result) => {
          const m = step.result === undefined ? null : mismatch(result, step.result, 'result');
          if (m) fail(at, m);
          if (typeof step.save === 'string') saved.set(step.save, result);
        });
      case 'font':
        return FontLibrary.default().add(fixture(step.file as string));
      case 'destroy':
        destroyed = true;
        return editor.destroy();
      case 'concurrent': {
        const all = (step.steps as Step[]).map((s, i) => start(s, `${at}.${i}`));
        return Promise.all(all);
      }
      default:
        return fail(at, `unknown step '${step.op}'`);
    }
  };

  /** Waits until the session is idle, including the commands that listeners submitted. */
  let destroyed = false;
  const idle = async () => {
    for (let n = 0; n !== background.length; ) {
      n = background.length;
      await Promise.all(background);
    }
    // Background layout runs on the engine's timers: to the end, so the page count is the document's.
    while (!destroyed && !editor.getState().pages.complete) await new Promise((r) => setTimeout(r, 5));
  };

  const runStep = async (raw: Step, at: string): Promise<void> => {
    if (failed && raw.op !== 'expectError') fail(failed.at, `failed: ${String(failed.error)}`);
    const step = (raw.op === 'repeat' ? raw : resolve(raw, at)) as Step;
    if (step.op.startsWith('expect')) {
      await idle();
      consistent(at);
    }
    switch (step.op) {
      case 'abort':
        abort = step.late === true ? 'late' : 'early';
        return;
      case 'expectState': {
        const m = mismatch(editor.getState(), step.match, 'state');
        if (m) fail(at, m);
        return;
      }
      case 'expectEvents': {
        const m = subsequence(events.splice(0), step.match as unknown[]);
        if (m) fail(at, m);
        return;
      }
      case 'expectError': {
        if (failed?.error.code !== step.code) fail(at, `expected the previous step to fail '${String(step.code)}', it ${failed ? `failed: ${String(failed.error)}` : 'succeeded'}`);
        failed = undefined;
        return;
      }
      case 'listen': {
        const off = editor.on(step.type as keyof TwipEvents, () => {
          off();
          background.push(command(step.command as Op).catch((e: unknown) => fail(`${at} (listener)`, String(e))));
        });
        return;
      }
      case 'repeat':
        for (let i = 0; i < (step.count as number); i++) await runStep(step.step as Step, `${at}, repeat ${i}`);
        return;
    }
    try {
      await start(step, at);
    } catch (error) {
      if (error instanceof Failure) throw error;
      if (!(error instanceof EditorError)) fail(at, String(error));
      failed = { error: error as EditorError, at };
    }
    consistent(at);
  };

  try {
    for (const [i, step] of script.steps.entries()) await runStep(step, `step ${i} (${step.op})`);
    if (failed) fail(failed.at, `failed: ${String(failed.error)}`);
    await idle();
  } finally {
    await editor.destroy();
  }
}

const files = existsSync(scripts) ? readdirSync(scripts).filter((f) => f.endsWith('.json')).sort() : [];
const pendingFile = new URL('pending.txt', scripts);
const pending = new Set(
  (existsSync(pendingFile) ? readFileSync(pendingFile, 'utf8') : '')
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l && !l.startsWith('#')),
);
// Scripts that are expected to pass run first, so a pending one that breaks the shared engine can't hide them.
files.sort((a, b) => Number(pending.has(a.replace(/\.json$/, ''))) - Number(pending.has(b.replace(/\.json$/, ''))));

/**
 * Runs a script in a worker thread: its own engine, so `"fonts": "empty"` gets a font library that holds only what the
 * script adds (a page has one library, which the other scripts share).
 */
function runAlone(file: string, script: Script): Promise<void> {
  return new Promise((resolve, reject) => {
    const worker = new Worker(new URL(import.meta.url), { workerData: { file, script } });
    worker.once('error', reject);
    worker.once('message', (failure: string | null) => {
      void worker.terminate();
      if (failure === null) resolve();
      else reject(new Failure(failure));
    });
  });
}

if (!isMainThread) {
  const { file, script } = workerData as { file: string; script: Script };
  run(file, script).then(
    () => parentPort!.postMessage(null),
    (e: unknown) => parentPort!.postMessage(e instanceof Error ? e.message : String(e)),
  );
} else test(`contract scripts (${RUNNER})`, { skip: files.length ? false : `no scripts in ${scripts.pathname}` }, async (t) => {
  if (!existsSync(engine)) throw new Error(`${RUNNER}: dist/twip-wasm.js is missing: build the engine (moon run twip-wasm:build), then this package`);
  for (const file of files) {
    const script = JSON.parse(readFileSync(new URL(file, scripts), 'utf8')) as Script;
    const name = file.replace(/\.json$/, '');
    const input = script.steps.some(sendsInput);
    const go = script.fonts === 'empty' ? runAlone : run;
    await t.test(name, async (st) => {
      if (input) return st.skip('sends input, which a headless editor takes none of');
      if (!pending.has(name)) return go(file, script);
      try {
        await go(file, script);
      } catch (e) {
        return st.skip(`pending: ${e instanceof Error ? e.message : String(e)}`);
      }
      throw new Error(`${RUNNER}: ${name}: passes, but pending.txt lists it: delete its line`);
    });
  }
});

// Commands are typed by id: a script's op is runnable as `editor.commands[id](arg)`.
export type CommandMethod = Commands[keyof Commands];
