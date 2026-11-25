"""Equality audit: every place the engine compares a model type that holds formatting.

In a scratch copy of the workspace, `PartialEq` and `Eq` are removed from the types in `types.txt`; the compiler then
reports every comparison of one, in library and test code. A type elsewhere whose own derive fails because it holds
one of them is stripped too, and the check runs again until nothing new appears, so the users of containing types are
listed as well.

`sites.txt` holds each site with the relation it needs:

    L        layout equivalence: would layout produce the same result
    A        authored equality, but it gets layout equivalence today
    AL       both (joining text into one run)
    none     the values compared hold no formatting (markers, ids)
    derive   a derived `PartialEq` on a containing type; its users are listed under that type

A site is keyed by file, enclosing function, compared type and source line, so it survives edits elsewhere in the
file. The check fails when a site is new, gone, or unclassified (`?`), or when a type in FORBIDDEN is compared.

Run: python3 -I crates/twip/tests/equality/audit.py [--write]
`--write` rewrites sites.txt, keeping the relation of every site it still finds and marking new ones `?`.
"""

import collections
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
WORK = REPO / "target" / "equality-audit"
COPY = WORK / "workspace"
RELATIONS = {"L", "A", "AL", "none", "derive", "?"}
# Formats no site may compare with `==`: each has named equality operations (`layout_eq`, `authored_eq`,
# `same_run`). The frozen XML resolution baseline keeps its comparisons as they were.
FORBIDDEN = {"RunStyle"}
BASELINE = {"crates/twip/src/docx/resolve.rs", "crates/twip/src/docx/props.rs", "crates/twip/src/docx/styles.rs", "crates/twip/src/docx/numbering.rs"}
COMPARISON = re.compile(r"^binary operation `[!=]=` cannot be applied|^can't compare|`(?:Partial)?Eq`|trait bounds were not satisfied")
ITEM = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum)\s+(\w+)")
FN = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+(\w+)")


def read_types():
    out = set()
    for line in (HERE / "types.txt").read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if line:
            path, *names = line.split()
            out.update((path, n) for n in names)
    return out


def sync():
    """Copies the workspace's sources into the scratch folder, leaving its build output in place (modification times
    are kept, so cargo rebuilds only what changed)."""
    COPY.mkdir(parents=True, exist_ok=True)
    for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml"):
        shutil.copy2(REPO / name, COPY / name)
    skip = shutil.ignore_patterns("target", "bin", "obj", "node_modules")
    for name in ("crates", ".cargo"):
        shutil.rmtree(COPY / name, ignore_errors=True)
        shutil.copytree(REPO / name, COPY / name, ignore=skip)


def strip(types):
    """Removes PartialEq and Eq from the derives of `types`, (path, name) pairs relative to the workspace."""
    by_file = collections.defaultdict(set)
    for path, name in types:
        by_file[path].add(name)
    missing = set(types)
    for path, names in by_file.items():
        file = COPY / path
        lines = file.read_text(encoding="utf-8").split("\n")
        for i, line in enumerate(lines):
            m = ITEM.match(line)
            if not m or m.group(1) not in names:
                continue
            j = i - 1
            while j >= 0 and lines[j].lstrip().startswith(("#[", "///", "//")):
                attr = lines[j].strip()
                if attr.startswith("#[derive(") and attr.endswith(")]"):
                    kept = [t.strip() for t in attr[len("#[derive("):-2].split(",") if t.strip() not in ("PartialEq", "Eq")]
                    indent = lines[j][: len(lines[j]) - len(lines[j].lstrip())]
                    lines[j] = f"{indent}#[derive({', '.join(kept)})]" if kept else ""
                    missing.discard((path, m.group(1)))
                j -= 1
        # A hand-written `impl PartialEq for` the type goes too, to its closing brace.
        out, skip = [], False
        for line in lines:
            m = re.match(r"^impl (?:Partial)?Eq for (\w+) \{", line)
            if m and m.group(1) in names:
                skip = not line.rstrip().endswith("}")
                missing.discard((path, m.group(1)))
                continue
            if skip:
                skip = line != "}"
                continue
            out.append(line)
        file.write_text("\n".join(out), encoding="utf-8")
    return missing


def local(span):
    """The span in the workspace's own sources: the span itself, or the macro call site it was expanded from."""
    while span is not None:
        if not span["file_name"].startswith("/") or span["file_name"].startswith(str(COPY)):
            return span
        span = (span.get("expansion") or {}).get("span")
    return None


def from_derive(span):
    """The derive's item, (path, type), when the span is inside `#[derive(PartialEq)]` or `#[derive(Eq)]`."""
    exp = span.get("expansion")
    while exp is not None:
        if exp.get("macro_decl_name") in ("#[derive(PartialEq)]", "#[derive(Eq)]"):
            path = exp["span"]["file_name"]
            lines = (COPY / path).read_text(encoding="utf-8").split("\n")
            for line in lines[exp["span"]["line_start"] - 1:]:
                m = ITEM.match(line)
                if m:
                    return path, m.group(1)
            return None
        exp = (exp.get("span") or {}).get("expansion")
    return None


def short(ty):
    return re.sub(r"\b(?:[a-z_][a-z0-9_]*::)+", "", ty)


def compared(message):
    names = re.findall(r"`([^`]+)`", message)
    if message.startswith("binary operation") and len(names) >= 2:
        return short(names[1])
    if message.startswith("can't compare") and names:
        return short(names[0])
    if names:
        return short(names[-1])
    return "?"


def enclosing_fn(lines, at):
    for line in reversed(lines[:at]):
        m = FN.match(line)
        if m:
            return m.group(1)
        if ITEM.match(line):
            break
    return "-"


def check():
    env = dict(os.environ, CARGO_TARGET_DIR=str(WORK / "target"))
    found = []
    derived = set()
    for target in (["--lib"], ["--tests"]):
        run = subprocess.run(["cargo", "check", "-p", "twip", *target, "--message-format=json"], cwd=COPY, env=env, capture_output=True, text=True)
        for line in run.stdout.splitlines():
            msg = json.loads(line)
            if msg.get("reason") != "compiler-message":
                continue
            m = msg["message"]
            if m["level"] != "error" or m["message"].startswith("aborting") or m["message"].startswith("could not compile"):
                continue
            if not COMPARISON.search(m["message"]) and not COMPARISON.search(m.get("rendered") or ""):
                sys.exit(f"the scratch build failed for another reason; fix it first:\n{m.get('rendered') or m['message']}")
            primary = next((s for s in m["spans"] if s["is_primary"]), None)
            if primary is None:
                continue
            item = from_derive(primary)
            if item:
                derived.add(item)
            span = local(primary)
            if span is None:
                continue
            path = span["file_name"].removeprefix(str(COPY) + "/")
            lines = (COPY / path).read_text(encoding="utf-8").split("\n")
            at = span["line_start"]
            ty = item[1] if item else compared(m["message"])
            func = "-" if item else enclosing_fn(lines, at)
            found.append((path, func, ty, " ".join(lines[at - 1].split()), "derive" if item else None))
    return found, derived


def audit():
    """The sites, as (path, function, type, line) keys, the derive sites among them, and the containing types found."""
    sync()
    types = read_types()
    missing = strip(types)
    if missing:
        sys.exit(f"types.txt names types that have no derive to strip: {sorted(missing)}")
    containing = set()
    while True:
        found, derived = check()
        containing |= derived
        new = derived - types
        if not new:
            break
        types |= new
        strip(new)
    # Library and test builds report the same library site twice; a key counts once.
    keys = {f[:4] for f in found}
    derives = {f[:4] for f in found if f[4]}
    for path, ty in containing:
        key = (path, "-", ty, "#[derive(PartialEq)]")
        keys.add(key)
        derives.add(key)
    return keys, derives, sorted(types - read_types())


def load():
    rows = []
    for line in (HERE / "sites.txt").read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        relation, path, func, ty, snippet = line.split("\t")
        rows.append((relation, (path, func, ty, snippet)))
    return rows


def main():
    write = "--write" in sys.argv[1:]
    found, derives, derived = audit()
    listed = load() if (HERE / "sites.txt").exists() else []
    relation = {}
    for rel, key in listed:
        relation.setdefault(key, rel)
    if write:
        header = "# relation\tfile\tfunction\ttype\tline (see audit.py for the relations)\n"
        body = "".join(f"{relation.get(key, 'derive' if key in derives else '?')}\t{chr(9).join(key)}\n" for key in sorted(found))
        (HERE / "sites.txt").write_text(header + body, encoding="utf-8", newline="\n")
        print(f"{len(found)} sites written; containing types also stripped: {', '.join(f'{p}:{t}' for p, t in derived) or 'none'}")
        return
    problems = []
    listed_keys = {key for _, key in listed}
    for key in sorted(set(found) - listed_keys):
        problems.append(f"new site, classify it in sites.txt: {chr(9).join(key)}")
    for key in sorted(listed_keys - set(found)):
        problems.append(f"site no longer found, remove it from sites.txt: {chr(9).join(key)}")
    for rel, key in listed:
        if rel not in RELATIONS:
            problems.append(f"unknown relation `{rel}`: {chr(9).join(key)}")
        elif rel == "?":
            problems.append(f"unclassified: {chr(9).join(key)}")
        compared = re.findall(r"\w+", key[2])
        if any(t in FORBIDDEN for t in compared) and rel != "derive" and key[0] not in BASELINE:
            problems.append(f"compares {key[2]} with `==`; use its named equality operation: {chr(9).join(key)}")
    for p in problems:
        print(p)
    print(f"{len(found)} sites, {len(problems)} problems")
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()
