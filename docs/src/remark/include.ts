// Fills a code block whose meta names a file (```ts file=../../examples/web/mount.ts) with that file, so the guides
// show exactly the examples CI type-checks and builds. `file=path#name` takes the lines between `#region name` and
// the next `#endregion`, dedented. Paths are relative to the Markdown file.
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

interface Node {
  type: string;
  meta?: string | null;
  value?: string;
  children?: Node[];
}

export default function include() {
  return (tree: Node, file: { path: string }) => {
    const visit = (node: Node) => {
      const m = node.type === 'code' ? /(?:^|\s)file=([^\s#]+)(?:#(\S+))?/.exec(node.meta ?? '') : null;
      if (m) node.value = excerpt(readFileSync(resolve(dirname(file.path), m[1]!), 'utf8'), m[2], m[1]!);
      node.children?.forEach(visit);
    };
    visit(tree);
  };
}

function excerpt(text: string, region: string | undefined, path: string): string {
  let lines = text.replace(/\r\n/g, '\n').trimEnd().split('\n');
  if (!region) return lines.join('\n');
  const start = lines.findIndex((line) => line.trim().replace(/^\/\/\s*/, '') === `#region ${region}`);
  const end = lines.findIndex((line, i) => i > start && /^\s*(\/\/\s*)?#endregion/.test(line));
  if (start < 0 || end < 0) throw new Error(`${path}: no region ${region}`);
  lines = lines.slice(start + 1, end);
  const indent = Math.min(...lines.filter((line) => line.trim()).map((line) => line.search(/\S/)));
  return lines.map((line) => line.slice(indent)).join('\n');
}
