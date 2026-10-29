import { createCommands, type CommandInfo, type Commands, type Op } from './commands.js';
import { EditorError } from './errors.js';
import type { TwipEvents } from './events.js';
import { FontLibrary, useEngine } from './fonts/library.js';
import { claim } from './host/dom.js';
import { mountHost, type Host } from './host/mount.js';
import { createMountedReview, createReview, type MountedReview, type Review } from './review.js';
import { createAccessibility, createShortcuts, type Accessibility, type Shortcuts } from './services/accessibility.js';
import { createDialogs, type Dialogs } from './services/dialogs.js';
import { createDataParts, createElements, type DataParts, type Elements, type MountedElements } from './services/elements.js';
import { createFind, type Find } from './services/find.js';
import { createMenus, type MenuAction, type Menus } from './services/menus.js';
import { createNavigation, createStyles, type Navigation, type Styles } from './services/navigation.js';
import { createRuler, type Ruler, type RulerModelSource, type RulerUnit, type RulerView } from './services/ruler.js';
import { createSpelling, type Dictionary, type Spelling } from './services/spelling.js';
import { createGestures, type GestureBinding, type Gestures, type View } from './services/view.js';
import type { EditorState } from './state.js';
import { directTransport } from './transport/direct.js';
import { chooseExecution, workerTransport } from './transport/worker.js';
import { decodeEvent, type FrameInfo, Kind, Method, type Input, type Message, type Transport } from './transport/protocol.js';
import type { Author, Color, OverlayColors, Warning, WordCount } from './types.js';

/** Options for a new session. */
export interface SessionOptions {
  /** Start read only. Default false. */
  readOnly?: boolean;
  /** Start in editing (the default) or design mode. */
  mode?: 'editing' | 'design';
  /** Who makes changes. */
  author?: Author;
  /** The fonts to draw with: share one per page, {@link FontLibrary.default} (the default). */
  fonts?: FontLibrary;
  /** The default language, BCP 47. Default `'en-US'`. */
  language?: string;
  /** Spelling dictionaries. */
  dictionaries?: Dictionary[];
  /** Turns tracking on after open, unless the document locks it. */
  trackChanges?: boolean;
}

/** Options for {@link Editor.mount}. */
export interface MountOptions extends SessionOptions {
  /** The document to open; blank when absent. */
  document?: Uint8Array;
  /** Where the engine runs. Default `'auto'`: a worker where supported. */
  execution?: 'auto' | 'main' | 'worker';
  /** The built-in rulers. Default off. */
  rulers?: boolean | { horizontal?: boolean; vertical?: boolean };
  /** The ruler unit. Default from `language`: inches for en-US, centimeters otherwise. */
  rulerUnit?: RulerUnit;
  /** Room beside the pages for balloons, CSS px. Default 0. */
  markupPaneWidth?: number;
  /** The zoom. Default 100. */
  zoom?: number | 'pageWidth' | 'wholePage';
  /** The background around the pages. Default `'#E6E6E6'`; the paper stays white. */
  background?: Color;
  /**
   * The caret's blink: ms per phase, 0 for a steady caret. Default 530, or steady while the platform asks for reduced
   * motion (`prefers-reduced-motion`), which the editor then follows.
   */
  caretBlink?: number;
  /** Stop blinking after this many ms without input, the caret left visible. Default: never. */
  caretBlinkStopAfter?: number;
  /**
   * The colours of what the editor draws over the document. Default: the engine's, or the system's colours while the
   * platform is in forced-colours (high contrast) mode, which the editor then follows.
   */
  overlayColors?: Partial<OverlayColors>;
  /**
   * The memory the editor keeps pages' drawing in, bytes: their recordings (default 32 MB). Past it, what was drawn
   * longest ago goes first.
   */
  paintBudget?: { recordings?: number };
  /**
   * The editor draws each attached balloon's leader line (from `balloonsLaidOut`'s placements) in the overlay colours'
   * `leaders` colour, so the application draws only the cards. Default false: the application draws them.
   */
  drawLeaders?: boolean;
  /** Aborts the mount; everything it created is released. */
  signal?: AbortSignal;
}

/** An exported document. */
export interface ExportResult {
  /** The file. */
  bytes: Uint8Array;
  /** The revision it captures; later edits don't change these bytes. */
  revision: number;
  /** Fidelity warnings. */
  warnings: Warning[];
}

function destroyed(): EditorError {
  return new EditorError('destroyed', 'the editor was destroyed');
}

function aborted(): EditorError {
  return new EditorError('aborted', 'the operation was aborted');
}

/** Bytes as standard base64, as the engine takes them. */
function base64(bytes: Uint8Array): string {
  let s = '';
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}

/** A request in the engine's JSON shape: bytes travel as base64. */
export function toWire(value: unknown): unknown {
  if (value instanceof Uint8Array) return base64(value);
  if (Array.isArray(value)) return value.map(toWire);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, toWire(v)]));
  return value;
}

/** What a command, batch, menu item or dialog apply commits: its record (none when nothing changed) and value. */
interface Commit {
  record: { revision: number } | null;
  value: unknown;
}

function deepFreeze<T>(value: T): T {
  if (value && typeof value === 'object' && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const v of Object.values(value)) deepFreeze(v);
  }
  return value;
}

/** One editor's session over a transport: the state cache, the listeners and the calls. Internal. */
export class Session {
  readonly transport: Transport;
  readonly id: number;
  state: EditorState;
  readonly catalog: readonly CommandInfo[];
  destroyed = false;
  readonly #subscribers = new Set<(state: EditorState) => void>();
  readonly #listeners = new Map<string, Set<(event: never) => void>>();
  /** Mounted hosts listen for frame and host requests here. */
  readonly hostListeners = new Set<(message: Message) => void>();
  /** A mounted host's view of an engine event before it is emitted: its viewport points become client points. */
  adjust: ((type: keyof TwipEvents, event: unknown) => unknown) | undefined;
  /** Run once on destroy, last added first. */
  readonly disposers: (() => void)[] = [];
  #closing: Promise<void> | undefined;
  /** Set by a fatal `error` event: later operations reject with it. */
  #fault: EditorError | undefined;
  /** Commits waiting for the state cache to reach their revision. */
  #waiting: { revision: number; resolve: () => void }[] = [];

  private constructor(transport: Transport, id: number, state: EditorState, catalog: readonly CommandInfo[]) {
    this.transport = transport;
    this.id = id;
    this.state = deepFreeze(state);
    this.catalog = deepFreeze(catalog);
    this.disposers.push(transport.listen(id, (m) => this.#receive(m)));
  }

  /** Creates a session on the page's engine: on this thread, or in the page's engine worker. */
  static async start(options: SessionOptions = {}, execution: 'main' | 'worker' = 'main'): Promise<Session> {
    const { readOnly, mode, author, language, trackChanges } = options;
    const transport = execution === 'worker' ? workerTransport() : await directTransport();
    useEngine(transport);
    const id = await transport.createSession({ readOnly, mode, author, language, trackChanges });
    try {
      const state = (await transport.call(id, Method.State)) as EditorState;
      const catalog = (await transport.call(id, Method.Catalog).catch((e: unknown) => {
        // shortcut: an engine without the catalog call gives an empty catalog; drop once every engine has it.
        if (e instanceof EditorError && e.code === 'unsupported') return [];
        throw e;
      })) as CommandInfo[];
      const session = new Session(transport, id, state, catalog);
      for (const dictionary of options.dictionaries ?? []) await session.call(Method.SpellingAddDictionary, dictionary);
      return session;
    } catch (e) {
      await transport.closeSession(id).catch(() => {});
      throw e;
    }
  }

  /** Rejects `destroyed` once destroyed, or with the fault after one, else runs `call`. */
  guard<T>(call: () => Promise<T>): Promise<T> {
    if (this.destroyed) return Promise.reject(destroyed());
    return this.#fault ? Promise.reject(this.#fault) : call();
  }

  /** A structured call, with the request in the engine's JSON shape. */
  call<T = void>(method: Method, request?: unknown): Promise<T> {
    return this.guard(() => this.transport.call(this.id, method, toWire(request)) as Promise<T>);
  }

  /** A call that commits: resolves with its value once the state cache holds the revision it committed. */
  async commit<T = void>(method: Method, request?: unknown): Promise<T> {
    const commit = await this.call<Commit | null>(method, request);
    const revision = commit?.record?.revision;
    if (revision !== undefined && this.state.revision < revision && !this.destroyed) {
      await new Promise<void>((resolve) => this.#waiting.push({ revision, resolve }));
    }
    return (commit?.value ?? undefined) as T;
  }

  run(op: Op): Promise<unknown> {
    return this.commit(Method.Run, op);
  }

  input(input: Input): Promise<number> {
    return this.guard(() => this.transport.input(this.id, input));
  }

  frame(width: number, height: number): Promise<FrameInfo | null> {
    return this.guard(() => this.transport.frame(this.id, width, height));
  }

  subscribe(listener: (state: EditorState) => void): () => void {
    this.#subscribers.add(listener);
    this.#isolate(() => listener(this.state));
    return () => this.#subscribers.delete(listener);
  }

  on<K extends keyof TwipEvents>(type: K, listener: (event: TwipEvents[K]) => void): () => void {
    let set = this.#listeners.get(type);
    if (!set) this.#listeners.set(type, (set = new Set()));
    set.add(listener as (event: never) => void);
    return () => set.delete(listener as (event: never) => void);
  }

  /** Whether anything listens to `type`. */
  listening(type: keyof TwipEvents): boolean {
    return (this.#listeners.get(type)?.size ?? 0) > 0;
  }

  emit<K extends keyof TwipEvents>(type: K, event: TwipEvents[K]): void {
    const listeners = [...(this.#listeners.get(type) ?? [])];
    if (type === 'error' && !listeners.length) console.error(event);
    for (const listener of listeners) {
      if (this.destroyed) return;
      if (type === 'error') {
        try {
          (listener as (event: TwipEvents[K]) => void)(event);
        } catch (e) {
          globalThis.reportError?.(e);
        }
      } else this.#isolate(() => (listener as (event: TwipEvents[K]) => void)(event));
    }
  }

  destroy(): Promise<void> {
    return (this.#closing ??= (async () => {
      this.destroyed = true;
      for (const { resolve } of this.#waiting.splice(0)) resolve();
      for (const dispose of this.disposers.splice(0).reverse()) this.#isolate(dispose);
      await this.transport.closeSession(this.id).catch(() => {});
    })());
  }

  /** Runs application code: an exception is reported on `error`, never thrown into the engine or other listeners. */
  #isolate(code: () => void): void {
    try {
      code();
    } catch (cause) {
      this.emit('error', new EditorError('internal', `a listener threw: ${String(cause)}`, { cause }));
    }
  }

  #receive(message: Message): void {
    if (this.destroyed) return;
    switch (message.kind) {
      case Kind.State: {
        this.state = deepFreeze(message.payload as EditorState);
        const revision = this.state.revision;
        for (const w of this.#waiting) if (w.revision <= revision) w.resolve();
        this.#waiting = this.#waiting.filter((w) => w.revision > revision);
        for (const listener of [...this.#subscribers]) if (!this.destroyed) this.#isolate(() => listener(this.state));
        break;
      }
      case Kind.Event: {
        const { type, event } = decodeEvent(message.payload);
        if (event instanceof EditorError && event.fatal) this.#fault = event;
        const adjusted = this.adjust && !(event instanceof EditorError) ? this.adjust(type, event) : event;
        this.emit(type, (adjusted instanceof EditorError ? adjusted : deepFreeze(adjusted)) as never);
        break;
      }
      default:
        for (const listener of [...this.hostListeners]) if (!this.destroyed) this.#isolate(() => listener(message));
    }
  }
}

const sessions = new WeakMap<HeadlessEditor, Session>();
let constructing: Session | undefined;

/** The session behind an editor. Internal: for the SDK's own hosts and tests. */
export function sessionOf(editor: HeadlessEditor): Session {
  const session = sessions.get(editor);
  if (!session) throw new TypeError('not an editor');
  return session;
}

/** Constructs an editor over `session`. */
function build<T extends HeadlessEditor>(session: Session, make: () => T): T {
  constructing = session;
  try {
    return make();
  } finally {
    constructing = undefined;
  }
}

/** A session with no DOM: Node, workers, tests and servers. */
export class HeadlessEditor {
  /** Every edit, one method per command. */
  readonly commands: Commands;
  /** Every command's id, label, group, shortcuts and undo grouping. */
  readonly catalog: readonly CommandInfo[];
  /** The context menu. */
  readonly menus: Menus;
  /** The dialogs. */
  readonly dialogs: Dialogs;
  /** Find and replace. */
  readonly find: Find;
  /** Spell checking. */
  readonly spelling: Spelling;
  /** The document's headings. */
  readonly navigation: Navigation;
  /** The document's styles. */
  readonly styles: Styles;
  /** Tracked changes and comments. */
  readonly review: Review;
  /** The merge designer's elements. */
  readonly elements: Elements;
  /** The custom XML data parts. */
  readonly data: DataParts;
  /** The ruler model. */
  readonly ruler: RulerModelSource;
  /** The accessibility tree. */
  readonly accessibility: Accessibility;
  /** Keyboard shortcuts. */
  readonly shortcuts: Shortcuts;

  /** Editors are made by {@link HeadlessEditor.create} and {@link Editor.mount}. */
  protected constructor() {
    const session = constructing;
    if (!session) throw new TypeError('use HeadlessEditor.create() or Editor.mount()');
    constructing = undefined;
    sessions.set(this, session);
    this.commands = createCommands((op) => session.run(op));
    this.catalog = session.catalog;
    this.menus = createMenus(session);
    this.dialogs = createDialogs(session);
    this.find = createFind(session);
    this.spelling = createSpelling(session);
    this.navigation = createNavigation(session);
    this.styles = createStyles(session);
    this.review = createReview(session);
    this.elements = createElements(session);
    this.data = createDataParts(session);
    this.ruler = { model: () => session.state.ruler };
    this.accessibility = createAccessibility(session);
    this.shortcuts = createShortcuts(session);
  }

  /** Creates a session with a blank document. */
  static async create(options?: SessionOptions): Promise<HeadlessEditor> {
    const session = await Session.start(options);
    return build(session, () => new HeadlessEditor());
  }

  /** Replaces the document, only after it was read successfully. Resolves with the read's warnings. */
  open(bytes: Uint8Array, options?: { signal?: AbortSignal }): Promise<{ warnings: Warning[] }> {
    const s = sessionOf(this);
    const signal = options?.signal;
    return s.guard(async () => {
      if (signal?.aborted) throw aborted();
      const token = await s.transport.openPrepare(s.id, bytes);
      // A task between the read and the replace, for an abort to land in.
      await new Promise((resolve) => setTimeout(resolve));
      if (signal?.aborted) {
        await s.transport.openDiscard(s.id, token).catch(() => {});
        throw aborted();
      }
      return { warnings: await s.guard(() => s.transport.openCommit(s.id, token)) };
    });
  }

  /** Replaces the document with a blank one. */
  newDocument(): Promise<void> {
    return sessionOf(this).call(Method.NewDocument);
  }

  /** Exports the document as DOCX or PDF, with the revision it captures. */
  export(options: { format: 'docx' | 'pdf'; signal?: AbortSignal }): Promise<ExportResult> {
    const s = sessionOf(this);
    const { format, signal } = options;
    return s.guard(async () => {
      const result = await s.transport.export(s.id, { format }, signal);
      if (signal?.aborted) throw aborted();
      return result;
    });
  }

  /** Runs several commands atomically: one transaction and one undo step, or nothing when one fails. */
  batch(ops: readonly Op[], options?: { label?: string }): Promise<void> {
    return sessionOf(this).commit(Method.Batch, { commands: ops, label: options?.label });
  }

  /** The latest state snapshot: synchronous and immutable. */
  getState(): EditorState {
    return sessionOf(this).state;
  }

  /** Calls `listener` with the current state now, then after every change. Returns the unsubscribe function. */
  subscribe(listener: (state: EditorState) => void): () => void {
    return sessionOf(this).subscribe(listener);
  }

  /** Listens to an event. Returns the unsubscribe function. */
  on<K extends keyof TwipEvents>(type: K, listener: (event: TwipEvents[K]) => void): () => void {
    return sessionOf(this).on(type, listener);
  }

  /** Makes the editor read only, or editable again, without reopening. */
  setReadOnly(value: boolean): Promise<void> {
    return this.#set(Method.SetReadOnly, value);
  }

  /** Switches between editing and design mode. */
  setMode(mode: 'editing' | 'design'): Promise<void> {
    return this.#set(Method.SetMode, mode);
  }

  /** Sets who makes the next changes. */
  setAuthor(author: Author): Promise<void> {
    return this.#set(Method.SetAuthor, author);
  }

  /** Counts words, characters, paragraphs, lines and pages. */
  wordCount(): Promise<WordCount> {
    return sessionOf(this).call(Method.WordCount);
  }

  /** Releases the editor. Idempotent; later calls reject `destroyed`. */
  destroy(): Promise<void> {
    return sessionOf(this).destroy();
  }

  /** A setting: a structured call whose response is `null`. */
  async #set(method: Method, value: unknown): Promise<void> {
    await sessionOf(this).call(method, value);
  }
}

/** An editor mounted in a page: it owns input, rendering, scrolling and its DOM inside the container. */
export class Editor extends HeadlessEditor {
  /** The element the editor was mounted in. */
  readonly container: HTMLElement;
  /** Where the engine runs. */
  readonly executionMode: 'main' | 'worker';
  /** The rulers: the model, the built-in rulers and the viewport mapping. */
  override readonly ruler: Ruler;
  /** Review, with the markup pane and balloons. */
  override readonly review: MountedReview;
  /** Elements, with drop targets and hit testing. */
  override readonly elements: MountedElements;
  /** The view. */
  readonly view: View;
  /** The pointer gestures. */
  readonly gestures: Gestures;
  readonly #focus: () => void;

  /** Editors are made by {@link Editor.mount}. */
  protected constructor() {
    const session = constructing!;
    super();
    const { container, host, rulerView, gestures } = mounting!;
    this.container = container;
    this.executionMode = mounting!.execution;
    this.#focus = host.focus;
    this.ruler = createRuler(session, rulerView);
    this.review = Object.assign(createReview(session), createMountedReview(session), {
      setMarkupPaneWidth: host.setMarkupPaneWidth,
      balloonLayer: host.balloonLayer,
      attachBalloon: host.attachBalloon,
    });
    const { hitTest, dragOver, dragLeave, drop } = host;
    this.elements = Object.assign(createElements(session), { hitTest, dragOver, dragLeave, drop });
    this.view = host.view;
    this.gestures = createGestures(session, gestures);
    // A paste from the system clipboard is the host's: the browser's clipboard is read here.
    (this as { commands: Commands }).commands = Object.freeze({ ...this.commands, paste: host.paste });
    // So is the context menu's Paste.
    const { invoke } = this.menus;
    (this as { menus: Menus }).menus = Object.freeze({
      ...this.menus,
      invoke: (action: MenuAction) => {
        const command = (action as unknown as { command?: { type?: string; as?: 'formatted' | 'text' } }).command;
        return command?.type === 'paste' ? host.paste(command.as ? { as: command.as } : undefined) : invoke(action);
      },
    });
  }

  /** Mounts an editor in `container`, which must be empty of other editors. Resolves when it accepts commands. */
  static async mount(container: HTMLElement, options: MountOptions = {}): Promise<Editor> {
    const release = claim(container);
    let session: Session | undefined;
    try {
      const execution = chooseExecution(options.execution);
      const abort = () => {
        if (options.signal?.aborted) throw new EditorError('aborted', 'the mount was aborted');
      };
      abort();
      session = await Session.start(options, execution);
      const s = session;
      s.disposers.push(release);
      abort();
      // The shortcuts the editor claims, read through the editor's own lookup once it exists.
      let lookup: ((keys: string) => unknown) | undefined;
      const host = await mountHost(container, s, options, (keys) => lookup?.(keys) != null);
      const [rulerView, gestures] = await Promise.all([s.call<RulerView>(Method.HostRulerView), s.call<GestureBinding[]>(Method.GesturesBindings)]);
      mounting = { container, execution, host, rulerView: s.adjust?.('rulerChanged', rulerView) as RulerView, gestures };
      const editor = build(s, () => new Editor());
      lookup = editor.shortcuts.lookup;
      const { rulers, rulerUnit } = options;
      if (rulers) await editor.ruler.setVisible(rulers === true ? { horizontal: true, vertical: true } : rulers);
      if (rulerUnit) await editor.ruler.setUnit(rulerUnit);
      if (options.document) await editor.open(options.document, { signal: options.signal });
      abort();
      await host.ready();
      abort();
      return editor;
    } catch (e) {
      if (session) await session.destroy();
      else release();
      throw e;
    } finally {
      mounting = undefined;
    }
  }

  /** Focuses the editor, keeping the selection. Mounting never takes focus by itself. */
  focus(): void {
    this.#focus();
  }
}

let mounting: { container: HTMLElement; execution: 'main' | 'worker'; host: Host; rulerView: RulerView; gestures: GestureBinding[] } | undefined;
