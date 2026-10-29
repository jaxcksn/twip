import { EditorError } from '../errors.js';
import { directTransport } from '../transport/direct.js';
import { Kind, Method, type Transport } from '../transport/protocol.js';
import { googleFonts } from './google.js';

/** A font the engine needs. */
export interface FontRequest {
  /** The family. */
  family: string;
  /** Bold. */
  bold: boolean;
  /** Italic. */
  italic: boolean;
}

/** A face a library holds. */
export interface FaceInfo {
  /** The family. */
  family: string;
  /** Bold. */
  bold: boolean;
  /** Italic. */
  italic: boolean;
}

/** Draws one family (optionally one style of it) with another. */
export interface SubstitutionRule {
  /** The family the document asks for. */
  from: string;
  /** Only the bold faces (or only the regular ones). */
  bold?: boolean;
  /** Only the italic faces (or only the upright ones). */
  italic?: boolean;
  /** The family to draw instead. */
  to: string;
}

/** Where fonts come from for a request: adds the faces it finds, and resolves whether it found the family. */
export type FontSource = (request: FontRequest, signal: AbortSignal) => Promise<boolean>;

type Resolver = (request: FontRequest, signal: AbortSignal) => Promise<Uint8Array | URL | null>;

let page: FontLibrary | undefined;
let appResolver: Resolver | undefined;
let google: FontSource | undefined;
/** The page's engines, first started first: this thread's and the engine worker's, once editors started them. */
const engines: Transport[] = [];
/** The library's changes so far, replayed into an engine that starts later. */
const changes: ((engine: Transport) => Promise<unknown>)[] = [];

/**
 * An engine an editor started, and the font requests it sends. Internal: the library's changes so far are replayed
 * into it, and calls made after this keep their order with the editors'.
 */
export function useEngine(transport: Transport): Transport {
  if (!engines.includes(transport)) {
    engines.push(transport);
    for (const apply of changes) report(apply(transport));
    // The library is the engine's, so its requests name no session.
    transport.listen(0, (m) => {
      if (m.kind === Kind.FontRequest) void answerFontRequest(m.payload as FontRequest);
    });
  }
  return transport;
}

/** Runs `use` on the page's first engine, loading the engine on this thread when no editor has started one. */
function withEngine<T>(use: (engine: Transport) => Promise<T>): Promise<T> {
  return engines[0] ? use(engines[0]) : directTransport().then((t) => use(engines[0] ?? useEngine(t)));
}

/** Applies a change to every engine and to the ones that start later; resolves with the first engine's answer. */
function change<T>(apply: (engine: Transport) => Promise<T>): Promise<T> {
  return withEngine((first) => {
    changes.push(apply);
    for (const e of engines) if (e !== first) report(apply(e));
    return apply(first);
  });
}

/** A library call: the font library is the engine's, so no session is named. Only `FontFamilies` changes nothing. */
function libraryCall<T = void>(method: Method, request?: unknown): Promise<T> {
  const call = (e: Transport) => e.call(0, method, request) as Promise<T>;
  return method === Method.FontFamilies ? withEngine(call) : change(call);
}

/** The failure of a call a synchronous method started: reported, as there is no promise to reject. */
function report(call: Promise<unknown>): void {
  call.catch((e: unknown) => console.error(e));
}

/** Fetches a font file. */
async function fetchFont(url: URL | string, signal?: AbortSignal): Promise<Uint8Array> {
  const response = await fetch(url, { signal });
  if (!response.ok) throw new EditorError('unavailable', `${String(url)}: HTTP ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}

/** Adds font bytes, or the file at a URL, to the page's library. */
async function addFont(source: Uint8Array | URL | string, signal?: AbortSignal): Promise<FaceInfo[]> {
  const bytes = source instanceof Uint8Array ? source : await fetchFont(source, signal);
  return change((e) => e.addFont(bytes));
}

/** With a source to ask, the engine requests missing faces instead of calling them unresolved at once. */
function askEngine(): void {
  report(libraryCall(Method.FontSetResolver, true));
}

const sameFamily = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: 'accent' }) === 0;

/**
 * Answers the engine's request for a font: the application's resolver first, then Google Fonts. When neither adds
 * the family, the engine marks it unresolved. Internal.
 */
export async function answerFontRequest(request: FontRequest): Promise<void> {
  // shortcut: the signal never fires; abort it when every session that asked for the font is destroyed.
  const signal = new AbortController().signal;
  const resolver = appResolver;
  const fromResolver: FontSource | undefined =
    resolver &&
    (async (r, sig) => {
      const found = await resolver(r, sig);
      return found !== null && (await addFont(found, sig)).some((face) => sameFamily(face.family, r.family));
    });
  for (const source of [fromResolver, google]) {
    try {
      if (source && (await source(request, signal))) return;
    } catch (e) {
      console.error(e);
    }
  }
  report(libraryCall(Method.FontMarkUnresolved, request));
}

/**
 * The fonts editors draw with. Share one per page: every editor on the page uses {@link FontLibrary.default}, and a
 * font added there reaches every editor, including ones already open.
 */
export class FontLibrary {
  private constructor() {}

  /** The page's library. */
  static default(): FontLibrary {
    return (page ??= new FontLibrary());
  }

  /** Adds font files: TTF, OTF and TTC bytes, or a URL to fetch them from. */
  add(source: Uint8Array | URL | string): Promise<FaceInfo[]> {
    return addFont(source);
  }

  /** Fetches fonts the engine asks for: resolve with the bytes, a URL, or null when there is none. */
  setResolver(resolver: (request: FontRequest, signal: AbortSignal) => Promise<Uint8Array | URL | null>): void {
    appResolver = resolver;
    askEngine();
  }

  /** Resolves missing fonts from Google Fonts, optionally only some families. */
  useGoogleFonts(options?: { families?: string[] }): void {
    google = googleFonts(addFont, options?.families);
    askEngine();
  }

  /** Draws families with others: `{ 'Times New Roman': 'Tinos' }`, or rules per style. */
  substitute(rules: Record<string, string> | SubstitutionRule[]): void {
    const list = Array.isArray(rules) ? rules : Object.entries(rules).map(([from, to]) => ({ from, to }));
    report(libraryCall(Method.FontSetSubstitutions, list));
  }

  /** The family for text whose font can't be found. */
  setDefault(family: string): void {
    report(libraryCall(Method.FontSetDefault, family));
  }

  /** Every family the library can draw now, for a font box. */
  families(): Promise<string[]> {
    return libraryCall(Method.FontFamilies);
  }
}
