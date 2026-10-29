"""Checks every contract script in this folder: valid JSON, a `name` equal to the file name, a fixture that exists,
only the step kinds README.md lists, files and saved results that exist, and only known command ids. Every name in
pending.txt (the scripts runners skip until their feature lands) must be a script here.

The command ids come from a command reference given as the argument: the command contract (Markdown, its tables
headed `| Command`) or the generated command wire fixture (JSON, its `type` values). Without an argument, the wire
fixture is used.
Run: python -I tests/contract/check.py [command reference]
"""

import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
FIXTURES = ROOT / "crates" / "twip" / "tests" / "fixtures"
WIRE = ROOT / "crates" / "twip" / "tests" / "wire" / "command.json"


def step_kinds():
    """The step kinds in the README's Steps table: the first word of each code span in a row's first cell."""
    text = (HERE / "README.md").read_text(encoding="utf-8")
    table = text.split("## Steps", 1)[1].split("\n## ", 1)[0]
    return {span.split()[0] for row in table.splitlines() if row.startswith("| `") for span in re.findall(r"`([^`]+)`", row.split("|")[1])}


def command_ids(path):
    text = path.read_text(encoding="utf-8")
    if path.suffix == ".json":
        return set(re.findall(r'"type"\s*:\s*"(\w+)"', text))
    ids, in_table = set(), False
    for line in text.splitlines():
        if not line.startswith("|"):
            in_table = False
        elif line.startswith("| Command"):
            in_table = True
        elif in_table and line.startswith("| `"):
            ids.update(re.findall(r"`(\w+)`", line.split("|")[1]))
    return ids


def check(path, kinds, commands):
    errors = []
    try:
        script = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as e:
        return [f"invalid JSON: {e}"]
    if script.get("name") != path.stem:
        errors.append(f"name {script.get('name')!r} isn't the file name {path.stem!r}")
    if "fixture" in script and not (FIXTURES / script["fixture"]).is_file():
        errors.append(f"no fixture {script['fixture']}")
    if script.get("fonts", "system") not in ("system", "empty"):
        errors.append(f"fonts {script['fonts']!r} isn't system or empty")
    saved = set()

    def command(c, where):
        if not isinstance(c, dict) or c.get("type") not in commands:
            errors.append(f"{where}: unknown command {c.get('type') if isinstance(c, dict) else c!r}")
        values(c, where)

    def values(v, where):
        """Checks `$file` and `$saved` references anywhere in a value."""
        if isinstance(v, dict):
            if "$file" in v and not (FIXTURES / v["$file"]).is_file():
                errors.append(f"{where}: no file {v['$file']}")
            if "$saved" in v and v["$saved"] not in saved:
                errors.append(f"{where}: nothing saved as {v['$saved']!r} before")
            for x in v.values():
                values(x, where)
        elif isinstance(v, list):
            for x in v:
                values(x, where)

    def steps(items, where):
        if not isinstance(items, list) or not items:
            errors.append(f"{where}: steps must be a non-empty list")
            return
        for n, s in enumerate(items):
            at = f"{where}[{n}]"
            op = s.get("op") if isinstance(s, dict) else None
            if op not in kinds:
                errors.append(f"{at}: unknown step kind {op!r}")
                continue
            if op == "command":
                command(s.get("command"), at)
            elif op == "batch":
                for k, c in enumerate(s.get("commands") or [None]):
                    command(c, f"{at}.commands[{k}]")
            elif op == "listen":
                command(s.get("command"), at)
            elif op == "concurrent":
                steps(s.get("steps"), f"{at}.steps")
            elif op == "repeat":
                if not isinstance(s.get("count"), int) or s["count"] < 1:
                    errors.append(f"{at}: count must be a positive integer")
                steps([s.get("step")], f"{at}.step")
            elif op == "font" and not (FIXTURES / str(s.get("file"))).is_file():
                errors.append(f"{at}: no font file {s.get('file')}")
            elif op == "call":
                method = str(s.get("method"))
                if method.startswith("@") and method[1:].split(".")[0] not in saved:
                    errors.append(f"{at}: nothing saved as {method[1:].split('.')[0]!r} before")
                if method.startswith("commands.") and method.split(".", 1)[1] not in commands:
                    errors.append(f"{at}: unknown command {method}")
            values({k: v for k, v in s.items() if k not in ("command", "commands")}, at)
            if op == "call" and "save" in s:
                saved.add(s["save"])

    steps(script.get("steps"), "steps")
    return errors


def main():
    reference = Path(sys.argv[1]) if len(sys.argv) > 1 else WIRE
    if not reference.is_file():
        sys.exit(f"no command reference at {reference}: pass the command contract or the command wire fixture")
    kinds, commands = step_kinds(), command_ids(reference)
    failed = 0
    scripts = sorted(HERE.glob("*.json"))
    for path in scripts:
        for error in check(path, kinds, commands):
            print(f"{path.name}: {error}")
            failed += 1
    names = {path.stem for path in scripts}
    pending = [line.strip() for line in (HERE / "pending.txt").read_text(encoding="utf-8").splitlines()]
    pending = [name for name in pending if name and not name.startswith("#")]
    for name in pending:
        if name not in names:
            print(f"pending.txt: no script {name}.json")
            failed += 1
    if len(pending) != len(set(pending)):
        print("pending.txt: a script is listed twice")
        failed += 1
    print(f"{len(scripts)} scripts ({len(pending)} pending), {len(kinds)} step kinds, {len(commands)} command ids, {failed} problems")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
