// The shared contract scripts (tests/contract/*.json, format in its README) against a mounted editor, runner
// `web-worker`: the engine in the page's worker. Each script also runs with the engine on the main thread, and the two
// runs must record the same events and states. Unlike the headless runner, a mounted editor takes input, so the
// scripts with `key`, `text` and `click` steps run here too (through the browser's real input).
// Scripts listed in tests/contract/pending.txt are skipped while they fail, and fail the run once they pass.
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { expect, test, type Browser } from '@playwright/test';

const RUNNER = 'web-worker';
const root = new URL('../../../', import.meta.url);
const scripts = new URL('tests/contract/', root);
const fixtures = new URL('crates/twip/tests/fixtures/', root);

interface Script {
  fixture?: string;
  fonts?: 'system' | 'empty';
  steps: ({ op: string } & Record<string, unknown>)[];
}

/** What a run recorded at each `expect*` step, for the two modes to compare, or the step that failed. */
interface Run {
  failure: string | null;
  records: { at: string; events: unknown[]; state: unknown }[];
}

/** An input step, sent by Playwright as real browser input. `at` is the client point of a click. */
type InputStep = { op: 'key'; key: string; mods?: string[] } | { op: 'text'; text: string } | { op: 'click'; at: { x: number; y: number }; mods?: string[] };

const modifier: Record<string, string> = { ctrl: 'Control', shift: 'Shift', alt: 'Alt', meta: 'Meta' };

/** Runs one script in a new page, mounted with `execution`. */
async function runIn(browser: Browser, name: string, script: Script, execution: 'main' | 'worker'): Promise<Run> {
  const context = await browser.newContext();
  const page = await context.newPage();
  try {
    page.on('pageerror', (e) => console.log(`${name} (${execution}): page error: ${e.message}`));
    await page.route('**/blank.html', (route) => route.fulfill({ contentType: 'text/html', body: '<!doctype html><meta charset="utf-8"><body style="margin:0">' }));
    await page.route('**/fixtures/**', (route) => {
      const file = new URL(decodeURIComponent(new URL(route.request().url()).pathname.replace(/^\/fixtures\//, '')), fixtures);
      return existsSync(file) ? route.fulfill({ body: readFileSync(file) }) : route.fulfill({ status: 404 });
    });
    await page.exposeFunction('twipInput', async (step: InputStep) => {
      const mods = (step.op === 'text' ? [] : (step.mods ?? [])).map((m) => modifier[m]!);
      if (step.op === 'text') return page.keyboard.insertText(step.text);
      if (step.op === 'key') return page.keyboard.press([...mods, step.key].join('+'));
      for (const m of mods) await page.keyboard.down(m);
      await page.mouse.click(step.at.x, step.at.y);
      for (const m of mods.reverse()) await page.keyboard.up(m);
    });
    await page.goto('/blank.html');
    return await page.evaluate(runScript, { name, script, execution, runner: RUNNER });
  } finally {
    await context.close();
  }
}

/**
 * The script's run, inside the page: the same steps and matching as the headless runner (test/contract.test.ts).
 * Self-contained, as Playwright sends it to the page as source.
 */
async function runScript({ name, script, execution, runner }: { name: string; script: Script; execution: string; runner: string }): Promise<Run> {
  type Step = { op: string } & Record<string, unknown>;
  const twip = await import('/web/dist/index.js' as string);
  const { fromOp } = await import('/web/dist/commands.js' as string);
  const { encodeEvent } = await import('/web/dist/transport/protocol.js' as string);
  const { EditorError, FontLibrary } = twip;
  const fixture = async (file: string) => {
    const response = await fetch(`/fixtures/${file}`);
    if (!response.ok) throw new Error(`no fixture ${file}`);
    return new Uint8Array(await response.arrayBuffer());
  };
  if (script.fonts !== 'empty') await FontLibrary.default().add('/font.ttf');

  const container = document.createElement('div');
  container.style.cssText = 'width:800px;height:600px;';
  document.body.append(container);
  const editor = await twip.Editor.mount(container, { execution });
  if (editor.executionMode !== execution) return { failure: `${runner}: ${name}: mounted in ${editor.executionMode}, not ${execution}`, records: [] };

  const events: unknown[] = [];
  const records: Run['records'] = [];
  const eventTypes = [
    'documentChanged', 'selectionChanged', 'linkActivated', 'placeholderActivated', 'contextMenu', 'dialogRequested',
    'hostCommand', 'shortcut', 'gesture', 'wordAdded', 'fontsChanged', 'elementActivated', 'elementsChanged',
    'fieldsUpdateRequested', 'revisionsChanged', 'commentsChanged', 'commentAnchors', 'balloonsLaidOut', 'rulerChanged',
    'tooltip', 'announcement', 'warning', 'error',
  ];
  for (const type of eventTypes) editor.on(type, (event: unknown) => events.push(encodeEvent(type, event)));
  let inconsistent: string | undefined;
  editor.on('documentChanged', (record: { revision: number }) => {
    const now = editor.getState().revision;
    if (now !== record.revision) inconsistent ??= `documentChanged for revision ${record.revision}, but getState() is at ${now}`;
  });
  const background: Promise<unknown>[] = [];
  const saved = new Map<string, unknown>();
  let abort: 'early' | 'late' | undefined;
  let exported: Uint8Array | undefined;
  let failed: { error: { code: string }; at: string } | undefined;

  class Failure extends Error {}
  const fail = (at: string, why: string): never => {
    throw new Failure(`${runner}: ${name}, ${at}: ${why}`);
  };
  const consistent = (at: string) => {
    if (inconsistent) fail(at, inconsistent);
  };

  const isObject = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
  const typeName = (v: unknown) => (v === null || v === undefined ? 'null' : Array.isArray(v) ? 'array' : typeof v);
  const items = (v: unknown): unknown[] => (Array.isArray(v) ? v.flatMap((e) => [e, ...(isObject(e) ? items(e.items) : [])]) : []);
  const equal = (a: unknown, b: unknown): boolean => {
    if (Array.isArray(a)) return Array.isArray(b) && a.length === b.length && a.every((x, i) => equal(x, b[i]));
    if (isObject(a)) {
      if (!isObject(b)) return false;
      const keys = new Set([...Object.keys(a), ...Object.keys(b)].filter((k) => a[k] !== undefined || b[k] !== undefined));
      return [...keys].every((k) => equal(a[k], b[k]));
    }
    return (a ?? null) === (b ?? null);
  };
  const assign = (wants: unknown[], got: unknown[], used: boolean[]): boolean => {
    if (wants.length === 0) return true;
    const [first, ...rest] = wants;
    for (const [i, g] of got.entries()) {
      if (used[i] || mismatch(g, first, '') !== null) continue;
      used[i] = true;
      if (assign(rest, got, used)) return true;
      used[i] = false;
    }
    return false;
  };
  const mismatch = (actual: unknown, expected: unknown, path: string): string | null => {
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
        const m = mismatch(actual[k], e, `${path}.${k}`);
        if (m) return m;
      }
      return null;
    }
    return Object.is(actual ?? null, expected) ? null : `${path}: expected ${show(expected)}, got ${show(actual)}`;
  };
  const subsequence = (got: unknown[], wants: unknown[]): string | null => {
    let next = 0;
    for (const [i, want] of wants.entries()) {
      const at = got.slice(next).findIndex((e) => mismatch(e, want, '') === null);
      if (at < 0) return `events[${i}]: no later event matches ${JSON.stringify(want)}; the events were ${JSON.stringify(got)}`;
      next += at + 1;
    }
    return null;
  };

  const command = (op: unknown) => {
    const [id, arg] = fromOp(op);
    return editor.commands[id](arg);
  };
  const resolve = async (v: unknown, at: string): Promise<unknown> => {
    if (Array.isArray(v)) return Promise.all(v.map((x) => resolve(x, at)));
    if (!isObject(v)) return v;
    if ('$file' in v) return fixture(v.$file as string);
    if ('$saved' in v) {
      const key = v.$saved as string;
      if (!saved.has(key)) fail(at, `nothing saved as ${key}`);
      let value = saved.get(key);
      for (const step of (v.path as unknown[] | undefined) ?? []) {
        const next: unknown =
          typeof step === 'string' ? (value as Record<string, unknown> | null)?.[step]
          : typeof step === 'number' ? (value as unknown[] | null)?.[step]
          : items(value).find((e) => mismatch(e, step, '') === null);
        if (next === undefined) fail(at, `$saved ${key}: no ${JSON.stringify(step)} in ${JSON.stringify(value)}`);
        value = next;
      }
      return value;
    }
    return Object.fromEntries(await Promise.all(Object.entries(v).map(async ([k, x]) => [k, await resolve(x, at)])));
  };
  const signalFor = <T,>(use: (signal: AbortSignal | undefined) => Promise<T>): Promise<T> => {
    const how = abort;
    abort = undefined;
    if (how === 'early') return use(AbortSignal.abort());
    if (how !== 'late') return use(undefined);
    const late = new AbortController();
    return use(late.signal).finally(() => late.abort());
  };
  const call = async (method: string, args: unknown[], at: string): Promise<unknown> => {
    const path = method.split('.');
    const first = path.shift()!;
    let target: unknown = first.startsWith('@') ? saved.get(first.slice(1)) : first === 'fonts' ? FontLibrary.default() : editor[first];
    let owner: unknown = editor;
    for (const key of path) {
      owner = target;
      target = (target as Record<string, unknown> | undefined)?.[key];
    }
    if (target === undefined) fail(at, `no member ${method}`);
    return typeof target === 'function' ? await (target as (...a: unknown[]) => unknown).apply(owner, args) : target;
  };
  const frames = (n: number) => new Promise<void>((done) => {
    const next = () => (n-- > 0 ? requestAnimationFrame(next) : done());
    next();
  });
  /** Real input, from Playwright, then a call queued behind it: the step ends once the engine took the input. */
  const input = async (step: Step, at: string) => {
    await send(step, at);
    await editor.wordCount();
  };
  const send = async (step: Step, at: string) => {
    editor.focus();
    if (step.op !== 'click') return (window as any).twipInput(step);
    // shortcut: the text's last character, not its middle: find selects the text, and the input element sits at the
    // caret, the selection's end. The middle needs the selection's rectangle, which no state or request carries.
    const found = await editor.find.next({ text: step.text });
    if (!found.found) fail(at, `no ${String(step.text)} to click`);
    await frames(3);
    const end = container.querySelector('textarea')!.getBoundingClientRect();
    return (window as any).twipInput({ op: 'click', mods: step.mods, at: { x: end.left - 3, y: end.top + end.height / 2 } });
  };

  const start = (step: Step, at: string): Promise<unknown> => {
    switch (step.op) {
      case 'open':
        if (!script.fixture) fail(at, 'the script has no fixture');
        return fixture(script.fixture!).then((bytes) => signalFor((signal) => editor.open(bytes, { signal }))).then((result) => {
          const m = step.match === undefined ? null : mismatch(result, step.match, 'open');
          if (m) fail(at, m);
        });
      case 'openBytes':
        return signalFor((signal) => editor.open(Uint8Array.from(atob(step.base64 as string), (c) => c.charCodeAt(0)), { signal }));
      case 'reopen':
        if (!exported) fail(at, 'reopen before any docx export');
        return signalFor((signal) => editor.open(exported!, { signal }));
      case 'export':
        return signalFor((signal) => editor.export({ format: step.format, signal })).then((result: { bytes: Uint8Array; revision: number; warnings: unknown }) => {
          const now = editor.getState().revision;
          if (result.revision !== now) fail(at, `the export reports revision ${result.revision}, the state is at ${now}`);
          const head = String.fromCharCode(...result.bytes.subarray(0, 4));
          const m = step.match === undefined ? null : mismatch({ revision: result.revision, warnings: result.warnings, head }, step.match, 'export');
          if (m) fail(at, m);
          if (step.format === 'docx') exported = result.bytes;
        });
      case 'command':
        return command(step.command);
      case 'batch':
        return editor.batch(step.commands, step.label === undefined ? undefined : { label: step.label });
      case 'call':
        return call(step.method as string, (step.args as unknown[] | undefined) ?? [], at).then((result) => {
          const m = step.result === undefined ? null : mismatch(result, step.result, 'result');
          if (m) fail(at, m);
          if (typeof step.save === 'string') saved.set(step.save, result);
        });
      case 'font':
        return fixture(step.file as string).then((bytes) => FontLibrary.default().add(bytes));
      case 'destroy':
        destroyed = true;
        return editor.destroy();
      case 'key':
      case 'text':
      case 'click':
        return input(step, at);
      case 'concurrent':
        return Promise.all((step.steps as Step[]).map((s, i) => start(s, `${at}.${i}`)));
      default:
        return fail(at, `unknown step '${step.op}'`);
    }
  };
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
    const step = (raw.op === 'repeat' || raw.op === 'concurrent' ? raw : await resolve(raw, at)) as Step;
    if (step.op.startsWith('expect')) {
      await idle();
      consistent(at);
      // `rulerChanged` and `commentAnchors` go out with the frames, whose timing (and so the scroll offsets drawn)
      // differs between the modes: the scripts check them, the comparison doesn't.
      const framed = events.filter((e) => !['rulerChanged', 'commentAnchors'].includes((e as { type?: string }).type ?? ''));
      // `sequence` counts the states, and background layout makes as many as its timing does.
      records.push({ at, events: framed, state: { ...editor.getState(), sequence: 0 } });
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
      case 'expectError':
        if (failed?.error.code !== step.code) fail(at, `expected the previous step to fail '${String(step.code)}', it ${failed ? `failed: ${String(failed.error)}` : 'succeeded'}`);
        failed = undefined;
        return;
      case 'listen': {
        const off = editor.on(step.type, () => {
          off();
          background.push(command(step.command).catch((e: unknown) => fail(`${at} (listener)`, String(e))));
        });
        return;
      }
      case 'repeat':
        for (let i = 0; i < (step.count as number); i++) await runStep(step.step as Step, `${at}, repeat ${i}`);
        return;
    }
    try {
      // A concurrent step's operations are resolved first, so all of them are submitted in one go.
      await start(step.op === 'concurrent' ? { ...step, steps: await resolve(step.steps, at) } : step, at);
    } catch (error) {
      if (error instanceof Failure) throw error;
      if (!(error instanceof EditorError)) fail(at, String(error));
      failed = { error: error as { code: string }, at };
    }
    consistent(at);
  };

  try {
    for (const [i, step] of script.steps.entries()) await runStep(step, `step ${i} (${step.op})`);
    if (failed) fail(failed.at, `failed: ${String(failed.error)}`);
    await idle();
    // Times differ between the runs.
    return { failure: null, records: JSON.parse(JSON.stringify(records, (k, v) => (k === 'time' ? typeof v : v))) };
  } catch (e) {
    return { failure: e instanceof Error ? e.message : String(e), records: [] };
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

for (const file of files) {
  const name = file.replace(/\.json$/, '');
  test(`${name} (${RUNNER})`, async ({ browser }) => {
    test.setTimeout(120_000);
    const script = JSON.parse(readFileSync(new URL(file, scripts), 'utf8')) as Script;
    const worker = await runIn(browser, name, script, 'worker');
    const main = worker.failure ? null : await runIn(browser, name, script, 'main');
    const failure = worker.failure ?? main!.failure;
    if (pending.has(name)) {
      test.skip(failure !== null, `pending: ${failure}`);
      throw new Error(`${RUNNER}: ${name}: passes, but pending.txt lists it: delete its line`);
    }
    expect(failure).toBeNull();
    // The two modes: the same events and states at every expect step.
    expect(worker.records).toEqual(main!.records);
  });
}
