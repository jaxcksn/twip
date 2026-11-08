import { EditorError } from '../errors.js';
import { directTransport } from '../transport/direct.js';
import { Kind, Method, type Transport } from '../transport/protocol.js';

/** A font table's general class of a family: `roman` (serif), `swiss` (sans serif), `modern` (fixed pitch), and so on. */
export type FontClass = 'roman' | 'swiss' | 'modern' | 'script' | 'decorative' | 'auto';

/** A font table's pitch of a family. */
export type FontPitch = 'fixed' | 'variable' | 'default';

/** A document's description of a font family, from its font table. Each field is absent when the document doesn't state it. */
export interface FontDescription {
  /** The general class (`w:family`). */
  class?: FontClass;
  /** The pitch (`w:pitch`). */
  pitch?: FontPitch;
  /** The PANOSE classification (`w:panose1`), 20 hex digits. */
  panose?: string;
  /** The character set (`w:charset`). */
  charset?: number;
}

/** A font the engine needs, asked once per family and style. */
export interface FontRequest {
  /** The family, as the document names it. */
  family: string;
  /** Bold. */
  bold: boolean;
  /** Italic. */
  italic: boolean;
  /** The document's description of the family, when its font table has one. */
  description?: FontDescription;
  /** The generic category that description gives, whether or not classification is on. */
  proposed?: GenericFamily;
}

/**
 * What a resolver returns for a request: the font's bytes or a URL to fetch it from, a family to draw it with
 * (`{ family }`), the default of a generic category (`{ category }`), or `null` when there is none. A family or
 * category answer applies to every style of the family until aliases or replacements change.
 */
export type FontAnswer = Uint8Array | URL | string | { family: string } | { category: GenericFamily } | null;

/** A face a library holds. */
export interface FaceInfo {
  /** The family. */
  family: string;
  /** Bold. */
  bold: boolean;
  /** Italic. */
  italic: boolean;
}

/**
 * A draw-time replacement: `from` (optionally one style of it) is drawn with `to`, even when `from` is in the
 * library. The document keeps naming `from`. When `to` isn't in the library, `from` resolves as if there were no rule.
 */
export interface FontRule {
  /** The family the document asks for. */
  from: string;
  /** Only the bold faces (or only the regular ones). */
  bold?: boolean;
  /** Only the italic faces (or only the upright ones). */
  italic?: boolean;
  /** The family to draw instead. */
  to: string;
}

/** A generic font category, like the CSS generic families. */
export type GenericFamily = 'serif' | 'sansSerif' | 'monospace' | 'script' | 'decorative';

/**
 * An alias: when `from` (optionally one style of it) isn't in the library, it is drawn with `to`, another family or
 * the default family of a generic category. Aliases chain up to 8 hops. The document keeps naming `from`.
 */
export interface Alias {
  /** The family the document names, case-insensitive. */
  from: string;
  /** The family, or the generic category, that draws instead. */
  to: string | { category: GenericFamily };
  /** Only the bold faces (or only the regular ones). */
  bold?: boolean;
  /** Only the italic faces (or only the upright ones). */
  italic?: boolean;
}

/**
 * The family that draws a missing font of each generic category. Opt-in: every category is unset by default, and
 * with none set no missing font is drawn through a category.
 */
export interface GenericDefaults {
  /** The family for missing serif fonts. */
  serif?: string;
  /** The family for missing sans-serif fonts. */
  sansSerif?: string;
  /** The family for missing fixed-pitch fonts. */
  monospace?: string;
  /** The family for missing script fonts. */
  script?: string;
  /** The family for missing decorative fonts. */
  decorative?: string;
}

/** Which stand-in draws a missing family. */
export type StandIn =
  /** A replacement's target is missing, so the document's own family draws. */
  | 'original'
  /** The default family of its generic category. */
  | 'generic'
  /** The general fallback family. */
  | 'fallback'
  /** The first face added to the library (or the symbol fallback, in an empty library). */
  | 'lastResort';

/** A family, in one style, that draws with a stand-in. */
export interface MissingFont {
  /** The family as the document names it, or the replacement target when `by` is `'original'`. */
  family: string;
  /** The bold style. */
  bold: boolean;
  /** The italic style. */
  italic: boolean;
  /** The replacement target and alias targets tried, in order. */
  tried: string[];
  /** The generic category the family has, from the document's font table, an alias or a resolver. */
  category?: GenericFamily;
  /** The family that draws instead; absent when nothing draws (an empty library with no symbol fallback). */
  drawnWith?: string;
  /** Why that family draws. */
  by: StandIn;
  /** The resolver's failure, when it failed. */
  error?: string;
}

/** Characters no font has, by the family their runs name. */
export interface MissingCharacters {
  /** The family the runs name. */
  family: string;
  /** The distinct characters, in code point order, at most 64. */
  characters: string;
  /** More than 64 characters are missing. */
  truncated: boolean;
}

/**
 * The document's fonts the library can't draw as the document asks. Each list is sorted by family, then style.
 */
export interface FontsState {
  /** Families a resolver is fetching; they draw with a stand-in meanwhile. */
  pending: MissingFont[];
  /** Families nothing provided; they draw with a stand-in. */
  missing: MissingFont[];
  /**
   * Characters that neither their run's font, the character-fallback families nor the symbol fallback has; they draw
   * as their font's missing-glyph box.
   */
  characters: MissingCharacters[];
}

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
