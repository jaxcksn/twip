// Writes src/wire.generated.ts: the public types this SDK takes from the engine's wire schema
// (crates/twip/tests/wire-schema.json), with the engine's documentation. `node scripts/wire.ts --check` fails when the
// file isn't what the schema makes now; web:test runs it.
//
// ADOPT lists the schema types written here, by their Rust names; the SDK's modules re-export them. A type they refer
// to must be adopted, listed in INLINE (written out where it is used), or exported by one of the SDK's own modules.
import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

const ADOPT = [
  // fonts/library.ts
  'FontClass',
  'FontPitch',
  'FontDescription',
  'FontRequest',
  'FaceInfo',
  'FontRule',
  'GenericFamily',
  'Alias',
  'GenericDefaults',
  'StandIn',
  'MissingFont',
  'MissingCharacters',
  'FontsState',
];

/** Schema types written out at each use rather than named. */
const INLINE = ['AliasTarget'];

/** The schema's string formats, as this SDK's types. */
const FORMATS: Record<string, string> = { color: 'Color', base64: 'Uint8Array', 'date-time': 'string', char: 'string' };

const web = resolve(import.meta.dirname, '..');
const out = join(web, 'src/wire.generated.ts');
const schema = JSON.parse(readFileSync(join(web, '../../crates/twip/tests/wire-schema.json'), 'utf8')) as { types: Def[] };

type Ty =
  | { kind: 'boolean' | 'number' | 'string' | 'null' | 'json' }
  | { kind: 'integer'; min: number; max: number }
  | { kind: 'format'; format: string }
  | { kind: 'literal'; value: string }
  | { kind: 'nullable'; of: Ty }
  | { kind: 'array'; of: Ty; len?: number }
  | { kind: 'tuple'; items: Ty[] }
  | { kind: 'map'; key: Ty; value: Ty }
  | { kind: 'oneOf'; of: Ty[] }
  | { kind: 'ref'; name: string };
interface Field {
  name: string;
  docs?: string;
  ty: Ty;
  omitted: boolean;
  defaulted: boolean;
}
type Payload = { kind: 'unit' } | { kind: 'value' | 'spread'; ty: Ty } | { kind: 'fields'; fields: Field[] };
interface Variant {
  name: string;
  docs?: string;
  payload: Payload;
}
type Shape =
  | { kind: 'units'; variants: { name: string; docs?: string }[] }
  | { kind: 'object'; fields: Field[]; closed: boolean }
  | { kind: 'tagged'; tag: string; variants: Variant[] }
  | { kind: 'untagged' | 'external'; variants: Variant[] }
  | { kind: 'alias'; of: Ty };
interface Def {
  name: string;
  module: string;
  docs?: string;
  shape: Shape;
}

const defs = new Map(schema.types.map((d) => [d.name, d]));
const adopted = new Set(ADOPT);

/** The SDK's own exported types, by name: the module each is in. */
function ownTypes(): Map<string, string> {
  const found = new Map<string, string>();
  const walk = (dir: string) => {
    for (const f of readdirSync(dir)) {
      const p = join(dir, f);
      if (statSync(p).isDirectory()) walk(p);
      else if (p.endsWith('.ts') && p !== out) {
        for (const m of readFileSync(p, 'utf8').matchAll(/^export (?:interface|type) (\w+)/gm)) found.set(m[1], p);
      }
    }
  };
  walk(join(web, 'src'));
  return found;
}

const own = ownTypes();
const imports = new Map<string, Set<string>>();
const problems: string[] = [];

/** A name from one of the SDK's modules, imported into the generated file. */
function external(name: string, from: string): string {
  const path = own.get(name);
  if (!path) {
    problems.push(`${from} refers to ${name}: adopt it, inline it, or export it from an SDK module`);
    return name;
  }
  const spec = './' + relative(join(web, 'src'), path).replace(/\.ts$/, '.js');
  (imports.get(spec) ?? imports.set(spec, new Set()).get(spec)!).add(name);
  return name;
}

/** `ty` as TypeScript, `from` naming its use for errors. */
function ts(ty: Ty, from: string): string {
  switch (ty.kind) {
    case 'boolean':
      return 'boolean';
    case 'integer':
    case 'number':
      return 'number';
    case 'string':
      return 'string';
    case 'null':
      return 'null';
    case 'json':
      return 'unknown';
    case 'format': {
      const t = FORMATS[ty.format];
      return /^[A-Z]/.test(t) && t !== 'Uint8Array' ? external(t, from) : t;
    }
    case 'literal':
      return `'${ty.value}'`;
    case 'nullable':
      return `${ts(ty.of, from)} | null`;
    case 'array':
      return `${parens(ts(ty.of, from))}[]`;
    case 'tuple':
      return `[${ty.items.map((t) => ts(t, from)).join(', ')}]`;
    case 'map':
      return `{ [key: string]: ${ts(ty.value, from)} }`;
    case 'oneOf': {
      // A value over the selection: `null`, `'mixed'`, or the value (state.ts's Tri and Mixed).
      const [a, b, c] = ty.of;
      if (ty.of.length === 3 && a.kind === 'null' && b.kind === 'literal' && b.value === 'mixed') {
        return c.kind === 'boolean' ? external('Tri', from) : `${external('Mixed', from)}<${ts(c, from)}>`;
      }
      return ty.of.map((t) => ts(t, from)).join(' | ');
    }
    case 'ref': {
      if (adopted.has(ty.name)) return ty.name;
      if (INLINE.includes(ty.name)) return body(defs.get(ty.name)!, from);
      return external(ty.name, from);
    }
  }
}

const parens = (t: string) => (t.includes(' | ') || t.includes(' & ') ? `(${t})` : t);

/**
 * The engine's documentation in this SDK's terms: links become code (a unit variant its wire name), and `None` is
 * "absent" on an optional field, `null` elsewhere.
 */
function translate(text: string, optional: boolean): string {
  return text
    .replace(/\[`(\w+)::(\w+)`\]/g, (_, ty: string, v: string) => {
      const d = defs.get(ty);
      const wire = v[0].toLowerCase() + v.slice(1);
      return d?.shape.kind === 'units' && d.shape.variants.some((x) => x.name === wire) ? `\`'${wire}'\`` : `\`${v}\``;
    })
    .replace(/\[`([^`\]]+)`\](?:\([^)]*\))?/g, '`$1`')
    .replace(/`None`/g, optional ? 'absent' : '`null`');
}

/** Documentation as a JSDoc block at `indent`, wrapped to 120 columns. */
function doc(raw: string | undefined, indent: string, optional = false): string {
  if (!raw) return '';
  const text = translate(raw, optional);
  const width = 120 - indent.length - 3;
  const lines: string[] = [];
  for (const para of text.split(/\n\s*\n/)) {
    if (lines.length) lines.push('');
    let line = '';
    for (const word of para.split(/\s+/).filter(Boolean)) {
      if (line && line.length + 1 + word.length > width) {
        lines.push(line);
        line = word;
      } else line = line ? `${line} ${word}` : word;
    }
    if (line) lines.push(line);
  }
  if (lines.length === 1 && indent.length + 7 + lines[0].length <= 120) return `${indent}/** ${lines[0]} */\n`;
  return `${indent}/**\n${lines.map((l) => `${indent} *${l ? ' ' + l : ''}`).join('\n')}\n${indent} */\n`;
}

function field(f: Field, indent: string, owner: string): string {
  // A field the engine may leave out is optional here, and left out rather than `null`.
  const optional = f.omitted;
  const ty = optional && f.ty.kind === 'nullable' ? f.ty.of : f.ty;
  const key = /^[A-Za-z_$][\w$]*$/.test(f.name) ? f.name : `'${f.name}'`;
  return `${doc(f.docs, indent, optional)}${indent}${key}${optional ? '?' : ''}: ${ts(ty, `${owner}.${f.name}`)};\n`;
}

/** An object written on one line, without its fields' documentation. */
function inlineObject(fields: Field[], owner: string): string {
  const parts = fields.map((f) => {
    const ty = f.omitted && f.ty.kind === 'nullable' ? f.ty.of : f.ty;
    return `${f.name}${f.omitted ? '?' : ''}: ${ts(ty, `${owner}.${f.name}`)}`;
  });
  return `{ ${parts.join('; ')} }`;
}

function object(fields: Field[], owner: string, indent = '  '): string {
  return `{\n${fields.map((f) => field(f, indent, owner)).join('')}${indent.slice(2)}}`;
}

/** A definition's type, as written after `type X =` (or inline). */
function body(d: Def, from: string): string {
  const s = d.shape;
  switch (s.kind) {
    case 'units':
      return s.variants.map((v) => `'${v.name}'`).join(' | ');
    case 'alias':
      return ts(s.of, d.name);
    case 'object':
      return object(s.fields, d.name);
    case 'tagged':
      return s.variants
        .map((v) => {
          const tag = `{ ${s.tag}: '${v.name}' }`;
          const p = v.payload;
          if (p.kind === 'unit') return tag;
          if (p.kind === 'value') return `{ ${s.tag}: '${v.name}'; value: ${ts(p.ty, `${d.name}.${v.name}`)} }`;
          if (p.kind === 'spread') return `(${tag} & ${ts(p.ty, `${d.name}.${v.name}`)})`;
          return `(${tag} & ${object(p.fields, `${d.name}.${v.name}`)})`;
        })
        .join(' | ');
    case 'untagged':
    case 'external':
      return s.variants
        .map((v) => {
          const p = v.payload;
          const inner = p.kind === 'unit' ? 'null' : p.kind === 'fields' ? inlineObject(p.fields, d.name) : ts(p.ty, `${d.name}.${v.name}`);
          if (s.kind === 'untagged') return inner;
          return p.kind === 'unit' ? `'${v.name}'` : `{ ${v.name}: ${inner} }`;
        })
        .join(' | ');
  }
  void from;
}

function declaration(d: Def): string {
  const s = d.shape;
  if (s.kind === 'object') return `${doc(d.docs, '')}export interface ${d.name} ${object(s.fields, d.name)}\n`;
  if (s.kind === 'units' && s.variants.some((v) => v.docs)) {
    const members = s.variants.map((v) => `${doc(v.docs, '  ')}  | '${v.name}'`).join('\n');
    return `${doc(d.docs, '')}export type ${d.name} =\n${members};\n`;
  }
  return `${doc(d.docs, '')}export type ${d.name} = ${body(d, d.name)};\n`;
}

const missing = ADOPT.filter((n) => !defs.has(n));
if (missing.length) problems.push(`not in the schema: ${missing.join(', ')}`);
const decls = ADOPT.filter((n) => defs.has(n)).map((n) => declaration(defs.get(n)!));
if (problems.length) {
  console.error(problems.join('\n'));
  process.exit(1);
}
const importLines = [...imports]
  .sort(([a], [b]) => a.localeCompare(b))
  .map(([spec, names]) => `import type { ${[...names].sort().join(', ')} } from '${spec}';`);
const text = [
  '// Generated by scripts/wire.ts from crates/twip/tests/wire-schema.json: edit the engine\'s types, not this file.',
  ...importLines,
  '',
  decls.join('\n'),
].join('\n');

if (process.argv.includes('--check')) {
  const current = (() => {
    try {
      return readFileSync(out, 'utf8');
    } catch {
      return '';
    }
  })();
  if (current !== text) {
    console.error('src/wire.generated.ts is stale: run `node scripts/wire.ts`');
    process.exit(1);
  }
} else writeFileSync(out, text);
