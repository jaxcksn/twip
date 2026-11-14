import { EditorError } from '../errors.js';
import { directTransport } from '../transport/direct.js';
import { Kind, Method, type Transport } from '../transport/protocol.js';
import type { Alias, FaceInfo, FontRequest, FontRule, GenericDefaults, GenericFamily } from '../wire.generated.js';

export type {
  Alias,
  FaceInfo,
  FontClass,
  FontDescription,
  FontPitch,
  FontRequest,
  FontRule,
  FontsState,
  GenericDefaults,
  GenericFamily,
  MissingCharacters,
  MissingFont,
  StandIn,
} from '../wire.generated.js';

/**
 * What a resolver returns for a request: the font's bytes or a URL to fetch it from, a family to draw it with
 * (`{ family }`), the default of a generic category (`{ category }`), or `null` when there is none. A family or
 * category answer applies to every style of the family until aliases or replacements change.
 */
export type FontAnswer = Uint8Array | URL | string | { family: string } | { category: GenericFamily } | null;

type Resolver = (request: FontRequest, signal: AbortSignal) => Promise<FontAnswer>;

let page: FontLibrary | undefined;
let appResolver: Resolver | undefined;
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

/** The engine's form of a resolver's answer. */
type WireAnswer =
  | { kind: 'font' }
  | { kind: 'family'; family: string }
  | { kind: 'category'; category: GenericFamily }
  | { kind: 'missing' }
  | { kind: 'failed'; message: string };

/**
 * Answers the engine's request for a font with the application's resolver. Bytes or a URL are added first; a resolver
 * that throws, a fetch that fails, or bytes that aren't a font answer `failed`, which the sessions report. Internal.
 */
export async function answerFontRequest(request: FontRequest): Promise<void> {
  // shortcut: the signal never fires; abort it when every session that asked for the font is destroyed.
  const signal = new AbortController().signal;
  let answer: WireAnswer;
  try {
    const found = appResolver ? await appResolver(request, signal) : null;
    if (found === null) answer = { kind: 'missing' };
    else if (found instanceof Uint8Array || found instanceof URL || typeof found === 'string') {
      await addFont(found, signal);
      answer = { kind: 'font' };
    } else if ('family' in found) answer = { kind: 'family', family: found.family };
    else answer = { kind: 'category', category: found.category };
  } catch (e) {
    answer = { kind: 'failed', message: e instanceof Error ? e.message : String(e) };
  }
  report(libraryCall(Method.FontAnswer, { request, answer }));
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

  /**
   * Sets the resolver: the engine asks it, once per family and style, for a font a document uses that the library
   * doesn't have. The request carries the document's description of the family and the category it gives. Resolve
   * with the font's bytes or a URL, `{ family }` or `{ category }` to draw it with another family or a category's
   * default, or `null`. While it runs, the family is in `state.fonts.pending` and draws with a stand-in; a resolver
   * that throws reports the family missing with its error.
   */
  setResolver(resolver: (request: FontRequest, signal: AbortSignal) => Promise<FontAnswer>): void {
    appResolver = resolver;
    askEngine();
  }

  /**
   * Sets the draw-time replacements, replacing earlier ones: `{ 'Times New Roman': 'Tinos' }`, or rules per style. A
   * replacement is the first step of resolution and applies even when the replaced family is in the library.
   */
  setReplacements(rules: Record<string, string> | FontRule[]): void {
    const list = Array.isArray(rules) ? rules : Object.entries(rules).map(([from, to]) => ({ from, to }));
    report(libraryCall(Method.FontSetReplacements, list));
  }

  /**
   * Sets the aliases, replacing earlier ones: `{ Arial: 'Arimo' }`, or aliases per style or to a generic category
   * (`{ from: 'Garamond', to: { category: 'serif' } }`). An alias applies only when its family isn't in the library;
   * the document keeps naming it. Families a resolver answered, or found missing, are requested again.
   */
  setAliases(aliases: Record<string, string> | Alias[]): void {
    const list = Array.isArray(aliases) ? aliases : Object.entries(aliases).map(([from, to]) => ({ from, to }));
    report(libraryCall(Method.FontSetAliases, list));
  }

  /**
   * Sets the default family of each generic category, for a missing family of that category (from the document's
   * font table, an alias or a resolver answer). Opt-in: `{}` turns it off.
   */
  setGenericDefaults(defaults: GenericDefaults): void {
    report(libraryCall(Method.FontSetGenericDefaults, defaults));
  }

  /**
   * Sets whether a document's font table gives a missing family its generic category. On by default; it has no
   * effect until a generic default is set. Off, only aliases to a category and resolver answers give one.
   */
  setClassification(options: { fromDocument: boolean }): void {
    report(libraryCall(Method.FontSetClassification, options.fromDocument));
  }

  /**
   * Sets the face for characters no other font has, after the character-fallback families: `'bundled'` (the bundled
   * symbol font, the default), `'off'`, or a font's bytes or URL, whose first face draws them. It is never a family a
   * document can name. Rejects with `invalidArgument` for bytes holding no face, leaving the setting as it was.
   */
  async setSymbolFallback(font: 'bundled' | 'off' | Uint8Array | URL | string): Promise<void> {
    if (font === 'bundled' || font === 'off') return libraryCall(Method.FontSetSymbolFallback, font);
    const bytes = font instanceof Uint8Array ? font : await fetchFont(font);
    return change((e) => e.setSymbolFallback(bytes));
  }

  /** Sets the families searched, in order, for a character a run's font lacks, before the symbol fallback. */
  setCharacterFallback(families: string[]): void {
    report(libraryCall(Method.FontSetCharacterFallback, families));
  }

  /**
   * Sets the general fallback: the family that draws a font nothing else resolves (after replacements, aliases,
   * resolver answers and generic defaults). With `null`, or when it isn't in the library, the first face added draws.
   */
  setFallback(family: string | null): void {
    report(libraryCall(Method.FontSetFallback, family));
  }

  /** Every family the library can draw now, for a font box. */
  families(): Promise<string[]> {
    return libraryCall(Method.FontFamilies);
  }
}
