import type { Session } from '../editor.js';
import { Method } from '../transport/protocol.js';

/** Hunspell dictionaries for one language. */
export interface Dictionary {
  /** The BCP 47 language tag. */
  language: string;
  /** The `.aff` file. */
  aff: Uint8Array;
  /** The `.dic` file. */
  dic: Uint8Array;
}

/** Spell checking. */
export interface Spelling {
  /** Adds a language's dictionaries. */
  addDictionary(dictionary: Dictionary): Promise<void>;
  /** Turns checking on or off. */
  setEnabled(on: boolean): Promise<void>;
  /** The default language for text with no language. */
  setLanguage(language: string): Promise<void>;
  /** Ignores a word for this session. */
  ignore(word: string): Promise<void>;
  /** Adds a word to the user's dictionary: emits `wordAdded`, for the application to persist. */
  add(word: string): Promise<void>;
  /** Loads the user's words for a language; kept until that language's dictionary arrives. */
  loadUserWords(language: string, words: string[]): Promise<void>;
  /** Suggestions for a word. */
  suggestions(word: string, language?: string): Promise<string[]>;
  /** Selects the next misspelling (F7), or resolves `null` when there is none. */
  next(): Promise<{ word: string; suggestions: string[] } | null>;
}

/** Spell checking over a session. */
export function createSpelling(s: Session): Spelling {
  return {
    addDictionary: (dictionary) => s.call(Method.SpellingAddDictionary, dictionary),
    setEnabled: (on) => s.call(Method.SpellingSetEnabled, on),
    setLanguage: (language) => s.call(Method.SpellingSetLanguage, language),
    ignore: (word) => s.call(Method.SpellingIgnore, word),
    add: (word) => s.call(Method.SpellingAdd, word),
    loadUserWords: (language, words) => s.call(Method.SpellingLoadUserWords, { language, words }),
    suggestions: (word, language) => s.call(Method.SpellingSuggestions, { word, language }),
    next: () => s.call(Method.SpellingNext),
  };
}
