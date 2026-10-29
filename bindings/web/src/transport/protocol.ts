// The private boundary between the public classes and the engine. The engine may run on this thread
// (DirectTransport) or in the page's engine worker; both deliver the same message stream and run operations in one
// ordered queue, so the public classes don't know which one they talk to.
import { EditorError, type ErrorCode } from '../errors.js';
import type { TwipEvents } from '../events.js';
import type { FaceInfo } from '../fonts/library.js';
import type { AccessibleNode } from '../services/accessibility.js';
import type { Warning } from '../types.js';

/** Structured call numbers: `TwipMethod` in `twip.h`, the C ABI's `Method`. The two lists must match exactly. */
export const Method = {
  Run: 1,
  Batch: 2,
  State: 3,
  Catalog: 4,
  NewDocument: 5,
  SetReadOnly: 6,
  SetMode: 7,
  SetAuthor: 8,
  ContextMenu: 9,
  Invoke: 10,
  DialogOpen: 11,
  DialogUpdate: 12,
  DialogApply: 13,
  Find: 14,
  FindAll: 15,
  SelectMatch: 16,
  Replace: 17,
  ReplaceAll: 18,
  SpellingAddDictionary: 19,
  SpellingSetEnabled: 20,
  SpellingSetLanguage: 21,
  SpellingIgnore: 22,
  SpellingAdd: 23,
  SpellingLoadUserWords: 24,
  SpellingSuggestions: 25,
  SpellingNext: 26,
  Headings: 27,
  SelectHeading: 28,
  Styles: 29,
  Revisions: 30,
  Comments: 31,
  People: 32,
  Elements: 33,
  Element: 34,
  Structure: 35,
  SelectElement: 36,
  DataParts: 37,
  AccessibleTree: 38,
  ShortcutsRegister: 39,
  ShortcutsUnregister: 40,
  ShortcutsLookup: 41,
  GesturesBindings: 42,
  GesturesSet: 43,
  GesturesReset: 44,
  WordCount: 45,
  Copy: 46,
  Paste: 47,
  FontFamilies: 48,
  FontSetSubstitutions: 49,
  FontSetDefault: 50,
  FontMarkUnresolved: 51,
  HostSetBackground: 52,
  HostSetMarkupPaneWidth: 53,
  HostSetRulers: 54,
  HostSetRulerUnit: 55,
  HostSetTabSelector: 56,
  HostRulerView: 57,
  HostCommentAnchors: 58,
  HostHitTest: 59,
  HostDragOver: 60,
  HostDragLeave: 61,
  HostDrop: 62,
  HostRevealSelection: 63,
  HostVisiblePages: 64,
  StackBalloons: 65,
  OpenCommit: 66,
  OpenDiscard: 67,
  DialogSelectIssue: 68,
  FontSetResolver: 69,
  Placeholders: 70,
} as const;

/** A structured call number. */
export type Method = (typeof Method)[keyof typeof Method];

/** What the engine sends: callback kinds of the C ABI. */
export const Kind = {
  Event: 1,
  State: 2,
  RequestFrame: 3,
  SetTimer: 4,
  FontRequest: 5,
  HostRequest: 6,
} as const;

/** A pointer shape over the document: the engine's `Cursor`. */
export type Cursor =
  | 'default'
  | 'text'
  | 'pointer'
  | 'move'
  | 'resizeHorizontal'
  | 'resizeVertical'
  | 'resizeNorthWestSouthEast'
  | 'resizeNorthEastSouthWest'
  | 'crosshair'
  | 'notAllowed'
  | 'copy';

/** What a copy puts on the clipboard: the engine's `ClipboardData`. */
export interface ClipboardData {
  text: string;
  html?: string;
  /** The engine's own copy, for a paste back into it. */
  internal?: string;
}

/** Something only the view showing a session can do: the {@link Kind.HostRequest} payload. Content CSS px. */
export type HostRequest =
  | { type: 'cursor'; cursor: Cursor }
  /** Send every pointer event to the view until `release`. */
  | { type: 'capture' }
  | { type: 'release' }
  /** A cut or copy from the keyboard, for the system clipboard. */
  | { type: 'clipboard'; data: ClipboardData }
  /** The caret moved: for the IME and the system caret. */
  | { type: 'caret'; x: number; y: number; height: number; visible: boolean }
  /** Scroll so this rectangle shows. */
  | { type: 'scroll_to'; x: number; y: number; width: number; height: number }
  /** The accessibility tree changed: mirror it. */
  | { type: 'a11y'; tree: AccessibleNode };

/** One engine-to-SDK message. `payload` is the decoded JSON, or undefined for kinds without one. */
export interface Message {
  kind: number;
  sessionId: number;
  payload: unknown;
}

/** Raw input, forwarded to the engine as typed calls (no JSON). */
export type Input =
  | { type: 'key'; key: number; mods: number }
  | { type: 'text'; text: string }
  // phase: start 0, update 1, end 2; `text` is the composition so far.
  | { type: 'composition'; phase: number; text: string }
  | { type: 'focus'; focused: boolean }
  | { type: 'surface'; width: number; height: number; dpr: number }
  | { type: 'scroll'; x: number; y: number }
  | { type: 'wheel'; dx: number; dy: number; mods: number }
  // kind: down 0, move 1, up 2, cancel 3, leave 4. button: primary 0, secondary 1, middle 2 (DOM `PointerEvent.button`).
  | { type: 'pointer'; kind: number; x: number; y: number; button: number; clicks: number; mods: number };

/** What a frame reports: the content's size, CSS px, and the pages laid out so far. */
export interface FrameInfo {
  contentWidth: number;
  contentHeight: number;
  pages: number;
}

/** How a canvas gets its frames: the best of the page's WebGPU, WebGL 2 and CPU pixels. */
export type Backend = 'webgpu' | 'webgl' | 'cpu';

/** The engine, behind one ordered queue. Every method runs after every earlier call on the same transport. */
export interface Transport {
  /** Creates a session over the shared engine. */
  createSession(options: object): Promise<number>;
  /** Closes a session; later calls with its id fail `destroyed`. */
  closeSession(session: number): Promise<void>;
  /** A structured call: request and response are the engine's JSON shapes. */
  call(session: number, method: Method, request?: unknown): Promise<unknown>;
  /** Opens a document; resolves with the read's warnings. */
  open(session: number, bytes: Uint8Array, signal?: AbortSignal): Promise<Warning[]>;
  /**
   * Reads a document without replacing the open one, for an open that may still be aborted: resolves with a token
   * for {@link Transport.openCommit} or {@link Transport.openDiscard}.
   */
  openPrepare(session: number, bytes: Uint8Array): Promise<number>;
  /** Replaces the open document with a prepared one; resolves with the read's warnings. */
  openCommit(session: number, token: number): Promise<Warning[]>;
  /** Drops a prepared document: the open one stays. */
  openDiscard(session: number, token: number): Promise<void>;
  /** Exports the document. */
  export(
    session: number,
    options: { format: 'docx' | 'pdf' },
    signal?: AbortSignal,
  ): Promise<{ bytes: Uint8Array; revision: number; warnings: Warning[] }>;
  /** Adds a font to the shared library. */
  addFont(bytes: Uint8Array): Promise<FaceInfo[]>;
  /** Forwards input; a key resolves with its outcome (0 unhandled, 1 handled, 2 and up a host key). */
  input(session: number, input: Input): Promise<number>;
  /**
   * Gives a session the canvas its frames go to, on the best backend the page has. Rejects `unsupported` when the
   * canvas can't be drawn at all.
   */
  attachCanvas(session: number, canvas: HTMLCanvasElement): Promise<Backend>;
  /**
   * Draws a frame of `width` x `height` device pixels into the session's canvas (sizing it), or resolves null when
   * there is no canvas or nothing to draw.
   */
  frame(session: number, width: number, height: number): Promise<FrameInfo | null>;
  /** Receives a session's messages, after the operation that caused them and before its promise settles. */
  listen(session: number, listener: (message: Message) => void): () => void;
}

/** The engine's error shape. */
interface WireError {
  code: ErrorCode;
  message: string;
  details?: unknown;
}

/** Turns a response envelope (`{ ok }` or `{ error }`) into its value, or throws its error. */
export function unwrap(response: unknown): unknown {
  const envelope = response as { ok?: unknown; error?: WireError } | null;
  if (envelope && typeof envelope === 'object' && 'error' in envelope && envelope.error) throw toError(envelope.error);
  if (envelope && typeof envelope === 'object' && 'ok' in envelope) return envelope.ok;
  throw new EditorError('internal', `malformed engine response: ${JSON.stringify(response)}`);
}

/** The engine's error shape as an {@link EditorError}. */
export function toError(error: WireError): EditorError {
  return new EditorError(error.code, error.message, error.details == null ? undefined : { details: error.details });
}

/** An {@link EditorError} in the engine's error shape. */
export function fromError(error: EditorError): WireError {
  return error.details === undefined
    ? { code: error.code, message: error.message }
    : { code: error.code, message: error.message, details: error.details };
}

/**
 * An engine event as its public type and payload. Events travel as `{ type, ...fields }`; an event whose payload
 * isn't an object carries it as `value`, and `error` carries the error shape plus `fatal`.
 */
export function decodeEvent(wire: unknown): { type: keyof TwipEvents; event: unknown } {
  const { type, ...fields } = wire as { type: keyof TwipEvents } & Record<string, unknown>;
  if (type === 'error') {
    const { fatal, ...error } = fields as unknown as WireError & { fatal?: boolean };
    const e = toError(error);
    return { type, event: fatal ? new EditorError(e.code, e.message, { details: e.details, fatal }) : e };
  }
  return { type, event: 'value' in fields ? fields.value : fields };
}

/** The events whose payload always travels as `value`, an object or not: the engine's optional payloads. */
const valueEvents = new Set<keyof TwipEvents>(['tooltip']);

/** The inverse of {@link decodeEvent}. */
export function encodeEvent(type: keyof TwipEvents, event: unknown): unknown {
  if (event instanceof EditorError) return { type, ...fromError(event), fatal: event.fatal };
  const spread = event !== null && typeof event === 'object' && !Array.isArray(event) && !valueEvents.has(type);
  return spread ? { type, ...event } : { type, value: event };
}
