// Writes Twip.Core/Wire.generated.cs: the public records and enums this SDK takes from the engine's wire schema
// (crates/twip/tests/wire-schema.json), with the engine's documentation. `node scripts/wire.mts --check` fails when the
// file isn't what the schema makes now; dotnet:test runs it.
//
// ADOPT lists the schema types written here, by their Rust names. A type they refer to must be adopted, named in NAMES,
// or declared by the SDK itself. A record of at most POSITIONAL_MAX fields is positional: required parameters first,
// then those the engine may leave out, nullable and defaulting to null; a larger one has init properties.
import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const ADOPT = [
  // Fonts
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
  'ReplaceScope',
  // Enums
  'SectionBreakKind',
  'Direction',
  'DeleteUnit',
  'MoveUnit',
  'UnderlineStyle',
  'CaseChange',
  'LevelChange',
  'TabAlignment',
  'TabLeader',
  'PageNumberFormat',
  'CellShift',
  'VerticalDirection',
  'CellAlignment',
  'BorderEdges',
  'NumberFormat',
  'LevelAlignment',
  'FollowNumber',
  'ListApplyTo',
  'NumberingMode',
  'ShadingTarget',
  'ShadingPattern',
  'PageBorderOffset',
  'PageBorderDisplay',
  'PageBorderScope',
  'FrameWrap',
  'FrameWidthRule',
  'FrameHeightRule',
  'FrameHorizontalAlign',
  'FrameVerticalAlign',
  'FrameHorizontalRelative',
  'FrameVerticalRelative',
  'ObjectWrap',
  'HorizontalReference',
  'VerticalReference',
  'ObjectHorizontalAlignment',
  'ObjectVerticalAlignment',
  'WrapSide',
  'FlipAxis',
  'Arrange',
  'ControlKind',
  'ControlLock',
  'MarkupView',
  'CommandId',
  'DialogKind',
  'HostCommand',
  'Origin',
  'WarningCode',
  'CommandGroup',
  'UndoGrouping',
  'RevisionKind',
  'StyleDialogType',
  'TableStylePart',
  'BordersTab',
  'BorderSetting',
  'ManageStylesTab',
  'StyleSort',
  'FieldKind',
  'SpecialIndent',
  'ListGalleryKind',
  'StyleType',
  'StyleVisibility',
  'ReferenceResolution',
  'VerticalMerge',
  'AccessibleRole',
  'AccessibleMark',
  'AnnouncementKind',
  'GestureId',
  'SelectionKind',
  'Story',
  'HeaderFooterKind',
  'HeaderFooterType',
  'ObjectKind',
  'ElementKind',
  'RulerUnit',
  'TabSelector',
  'GradientShape',
  'LineCap',
  'LineJoin',
  'ArrowKind',
  'ArrowSize',
  'TextAnchor',
  'TextAutofit',
  'TextDirection',
  'DocumentAppearance',
  'MeasureDetail',
  'ObjectGrid',
  'FieldShading',
  'PressAction',
  // Records
  'Author',
  'Identity',
  'Dictionary',
  'TransactionRecord',
  'ChangeSummary',
  'Warning',
  'WordCount',
  'RichParagraph',
  'RichRun',
  'ShapeGalleryGroup',
];

/** Schema types this SDK names otherwise (Rust name: C# type). */
const NAMES: Record<string, string> = { Points: 'Length', Location: 'WarningLocation' };

/** Fields this SDK types its own way (`Type.field`: the C# type). */
const FIELDS: Record<string, string> = {};

/** Records written with init properties whatever their size, and positional ones whatever their size. */
const PROPERTIES: string[] = [];
const POSITIONAL: string[] = [];
const POSITIONAL_MAX = 8;

/** The schema's string formats, as this SDK's types. */
const FORMATS: Record<string, string> = { color: 'ColorValue', base64: 'ReadOnlyMemory<byte>', 'date-time': 'DateTimeOffset', char: 'string' };

const dotnet = resolve(import.meta.dirname, '..');
const core = join(dotnet, 'Twip.Core');
const out = join(core, 'Wire.generated.cs');
const schema = JSON.parse(readFileSync(join(dotnet, '../../crates/twip/tests/wire-schema.json'), 'utf8')) as { types: Def[] };

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
type Shape =
  | { kind: 'units'; variants: { name: string; docs?: string }[] }
  | { kind: 'object'; fields: Field[]; closed: boolean }
  | { kind: 'tagged' | 'untagged' | 'external' | 'alias' };
interface Def {
  name: string;
  docs?: string;
  shape: Shape;
}

const defs = new Map(schema.types.map((d) => [d.name, d]));
const adopted = new Set(ADOPT);
const problems: string[] = [];

/** The SDK's own public types, by name (nested namespaces qualified: `Hosting.EditorCursor`). */
function ownTypes(): Set<string> {
  const found = new Set<string>();
  const walk = (dir: string) => {
    for (const f of readdirSync(dir)) {
      const p = join(dir, f);
      if (f === 'bin' || f === 'obj') continue;
      if (statSync(p).isDirectory()) walk(p);
      else if (p.endsWith('.cs') && p !== out) {
        const text = readFileSync(p, 'utf8');
        const ns = /^namespace Twip(?:\.(\w+))?;/m.exec(text)?.[1];
        for (const m of text.matchAll(/^public (?:readonly )?(?:sealed |abstract |static )?(?:partial )?(?:record struct|record|enum|class|struct) (\w+)/gm)) {
          found.add(ns ? `${ns}.${m[1]}` : m[1]);
        }
      }
    }
  };
  walk(core);
  return found;
}

const own = ownTypes();

/** PascalCase of a wire name (`sansSerif`, `unsupported-element`). */
const pascal = (wire: string) => wire.split(/[-_]/).map((w) => w[0].toUpperCase() + w.slice(1)).join('');

/** What System.Text.Json's camelCase policy makes of a C# name. */
function camel(name: string): string {
  let i = 0;
  while (i < name.length && name[i] >= 'A' && name[i] <= 'Z') i++;
  if (i <= 1) return name[0].toLowerCase() + name.slice(1);
  // A run of capitals: all but the last, which starts the next word, unless the run ends the name.
  const run = i === name.length ? i : i - 1;
  return name.slice(0, run).toLowerCase() + name.slice(run);
}

/** A field's C# name: the wire name in PascalCase, `colour` spelled `Color`. */
const memberName = (wire: string) => pascal(wire).replace(/Colour/g, 'Color');

/** `ty` as C#, `from` naming its use for errors. */
function cs(ty: Ty, from: string): string {
  switch (ty.kind) {
    case 'boolean':
      return 'bool';
    case 'integer':
      return ty.max <= 4294967295 && ty.min >= -2147483648 ? 'int' : 'long';
    case 'number':
      return 'double';
    case 'string':
      return 'string';
    case 'json':
      return 'System.Text.Json.JsonElement';
    case 'format':
      return FORMATS[ty.format];
    case 'nullable': {
      const inner = cs(ty.of, from);
      return inner.endsWith('?') ? inner : `${inner}?`;
    }
    case 'array':
      return `IReadOnlyList<${cs(ty.of, from)}>`;
    case 'map':
      return `IReadOnlyDictionary<${ty.key.kind === 'ref' ? cs(ty.key, from) : 'string'}, ${cs(ty.value, from)}>`;
    case 'oneOf': {
      // A value over the selection: `null`, `'mixed'`, or the value.
      const [a, b, c] = ty.of;
      if (ty.of.length === 3 && a.kind === 'null' && b.kind === 'literal' && b.value === 'mixed') return `FormatValue<${cs(c, from)}>`;
      problems.push(`${from}: no C# type for one of ${JSON.stringify(ty.of)}: name one in FIELDS`);
      return 'object';
    }
    case 'ref': {
      if (adopted.has(ty.name)) return ty.name;
      if (NAMES[ty.name]) return NAMES[ty.name];
      if (own.has(ty.name)) return ty.name;
      problems.push(`${from} refers to ${ty.name}: adopt it, name it in NAMES, or declare it in the SDK`);
      return ty.name;
    }
    case 'null':
    case 'literal':
    case 'tuple':
      problems.push(`${from}: no C# type for ${ty.kind}: name one in FIELDS`);
      return 'object';
  }
}

const xml = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/**
 * The engine's documentation in C# terms: a field of the same record is a `paramref` (positional) or a `see cref`, a
 * known type a `see cref`, other code `<c>`, and `None` is null.
 */
function translate(text: string, owner: Def | undefined, positional: boolean): string {
  const fields = owner?.shape.kind === 'object' ? owner.shape.fields.map((f) => f.name) : [];
  const typeRef = (name: string) => (adopted.has(name) || own.has(name) ? name : NAMES[name]);
  return xml(text)
    .replace(/\[`(\w+)::(\w+)`\](?:\([^)]*\))?/g, (_, t: string, v: string) => {
      const name = typeRef(t);
      // A lowercase name is a field: its C# property.
      const member = /^[a-z]/.test(v) ? memberName(v) : v;
      return name ? `<see cref="${name}.${member}"/>` : `<c>${member}</c>`;
    })
    .replace(/\[`([^`\]]+)`\](?:\([^)]*\))?/g, (_, t: string) => {
      const name = typeRef(t);
      return name ? `<see cref="${name}"/>` : `<c>${t}</c>`;
    })
    .replace(/`None`/g, 'null')
    .replace(/`([^`]+)`/g, (_, code: string) => {
      if (fields.includes(code)) return positional ? `<paramref name="${memberName(code)}"/>` : `<see cref="${memberName(code)}"/>`;
      return `<c>${code}</c>`;
    });
}

/** `text` wrapped to 120 columns as `///` lines at `indent`, inside `tag` (one line when it fits). */
function docLines(text: string, indent: string, open: string, close: string): string {
  const one = `${indent}/// ${open}${text}${close}`;
  if (one.length <= 120 && !text.includes('\n\n')) return `${one}\n`;
  const width = 120 - indent.length - 4;
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
  return `${indent}/// ${open}\n${lines.map((l) => `${indent}///${l ? ' ' + l : ''}`).join('\n')}\n${indent}/// ${close}\n`;
}

function summary(raw: string | undefined, indent: string, owner: Def | undefined, positional: boolean, from: string): string {
  if (!raw) {
    problems.push(`${from} has no documentation`);
    return '';
  }
  return docLines(translate(raw, owner, positional), indent, '<summary>', '</summary>');
}

function enumDecl(d: Def & { shape: { kind: 'units' } }): string {
  const members = d.shape.variants
    .map((v) => {
      const name = pascal(v.name);
      const attr = camel(name) === v.name || d.shape.variants.length === 0 ? '' : `    [JsonStringEnumMemberName("${v.name}")]\n`;
      return `${summary(v.docs, '    ', d, false, `${d.name}.${v.name}`)}${attr}    ${name},\n`;
    })
    .join('');
  // Kebab-case names (`unsupported-element`) have their own converter.
  const kebab = d.shape.variants.some((v) => v.name.includes('-')) ? `[JsonConverter(typeof(Wire.KebabEnumConverter<${d.name}>))]\n` : '';
  return `${summary(d.docs, '', d, false, d.name)}${kebab}public enum ${d.name}\n{\n${members}}\n`;
}

interface Member {
  name: string;
  type: string;
  omitted: boolean;
  json: string;
  docs: string;
}

function members(d: Def & { shape: { kind: 'object' } }): Member[] {
  return d.shape.fields.map((f) => {
    const name = memberName(f.name);
    const path = `${d.name}.${f.name}`;
    let type = FIELDS[path] ?? cs(f.omitted && f.ty.kind !== 'nullable' ? { kind: 'nullable', of: f.ty } : f.ty, path);
    if (f.omitted && !type.endsWith('?')) type += '?';
    const attrs: string[] = [];
    if (f.omitted) attrs.push('JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)');
    if (camel(name) !== f.name) attrs.push(`JsonPropertyName("${f.name}")`);
    if (!f.docs) problems.push(`${path} has no documentation`);
    return { name, type, omitted: f.omitted, json: attrs.join(', '), docs: f.docs ?? '' };
  });
}

function recordDecl(d: Def & { shape: { kind: 'object' } }): string {
  const ms = members(d);
  const positional = POSITIONAL.includes(d.name) || (!PROPERTIES.includes(d.name) && ms.length <= POSITIONAL_MAX);
  if (positional) {
    const ordered = [...ms.filter((m) => !m.omitted), ...ms.filter((m) => m.omitted)];
    const params = ordered.map((m) => `${m.json ? `[property: ${m.json}] ` : ''}${m.type} ${m.name}${m.omitted ? ' = null' : ''}`);
    const docs = ordered.map((m) => docLines(translate(m.docs, d, true), '', `<param name="${m.name}">`, '</param>')).join('');
    const one = `public sealed record ${d.name}(${params.join(', ')});`;
    const decl = one.length <= 120 && ordered.length ? one : `public sealed record ${d.name}(\n${params.map((p) => `    ${p}`).join(',\n')});`;
    return `${summary(d.docs, '', d, true, d.name)}${docs}${ordered.length ? decl : `public sealed record ${d.name};`}\n`;
  }
  const props = ms
    .map((m) => `${docLines(translate(m.docs, d, false), '    ', '<summary>', '</summary>')}${m.json ? `    [${m.json}]\n` : ''}    public ${m.type} ${m.name} { get; init; }\n`)
    .join('\n');
  return `${summary(d.docs, '', d, false, d.name)}public sealed record ${d.name}\n{\n${props}}\n`;
}

const decls: string[] = [];
for (const name of ADOPT) {
  const d = defs.get(name);
  if (!d) problems.push(`${name} isn't in the schema`);
  else if (d.shape.kind === 'units') decls.push(enumDecl(d as Def & { shape: { kind: 'units' } }));
  else if (d.shape.kind === 'object') decls.push(recordDecl(d as Def & { shape: { kind: 'object' } }));
  else problems.push(`${name} is ${d.shape.kind}: this generator writes records and enums`);
}
for (const path of Object.keys(FIELDS)) {
  const [t, f] = path.split('.');
  const d = defs.get(t);
  if (!adopted.has(t) || d?.shape.kind !== 'object' || !d.shape.fields.some((x) => x.name === f)) problems.push(`FIELDS names ${path}, which no adopted record has`);
}
if (problems.length) {
  console.error(problems.join('\n'));
  process.exit(1);
}

const text = [
  "// Generated by scripts/wire.mts from crates/twip/tests/wire-schema.json: edit the engine's types, not this file.",
  '#nullable enable',
  'using System.Drawing;',
  'using System.Text.Json.Serialization;',
  '',
  'namespace Twip;',
  '',
  decls.join('\n'),
].join('\n');

if (process.argv.includes('--check')) {
  let current = '';
  try {
    current = readFileSync(out, 'utf8');
  } catch {}
  if (current !== text) {
    console.error('Twip.Core/Wire.generated.cs is stale: run `node scripts/wire.mts`');
    process.exit(1);
  }
} else writeFileSync(out, text);
