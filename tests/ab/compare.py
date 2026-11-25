"""Base against head: what two builds of the engine produce from the same inputs, compared byte for byte.

    geometry   every fixture's laid-out pages (`twip-tools dump`), glyph positions included
    saves      every fixture saved unedited and after each edit family (`twip-tools roundtrip --edits all --keep`)
    fidelity   the round-trip gate's saved packages (`TWIP_FIDELITY_OUT`)
    scenarios  what each editor scenario saves (`TWIP_SCENARIO_OUT`)
    reuse      layout reuse counts over a fixed edit script (`TWIP_REUSE_OUT`)

Both builds read the head's fixtures. The base is built in its own worktree (target/ab/base) with its own target
folder, so the two builds never share one. A comparison the base cannot produce (its tests have no such hook) is
reported and skipped.

Run: python3 -I tests/ab/compare.py <base revision> [--only geometry,saves,...]
Exits 1 when any output differs.
"""

import filecmp
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
WORK = REPO / "target" / "ab"
FIXTURES = REPO / "crates" / "twip" / "tests" / "fixtures"
ALL = ["geometry", "saves", "fidelity", "scenarios", "reuse"]
TESTS = {"fidelity": "tests::fidelity", "scenarios": "editing::tests::scenarios", "reuse": "editing::tests::reuse"}


def sh(args, cwd, env=None, check=True, out=None):
    print("+", " ".join(str(a) for a in args), flush=True)
    with open(out, "w") if out else open(os.devnull, "w") as sink:
        r = subprocess.run(args, cwd=cwd, env=env, stdout=sink if out else None)
    if check and r.returncode != 0:
        sys.exit(f"failed ({r.returncode}): {' '.join(str(a) for a in args)}")
    return r.returncode


def worktree(rev):
    path = WORK / "base"
    if not (path / ".git").exists():
        sh(["git", "worktree", "add", "--detach", "--force", str(path), rev], REPO)
    else:
        sh(["git", "checkout", "--detach", "--force", rev], path)
    return path


def produce(side, src, what):
    out = WORK / side
    env = dict(os.environ, CARGO_TARGET_DIR=str(WORK / f"{side}-target"))
    tools = WORK / f"{side}-target" / "release" / "twip-tools"
    if {"geometry", "saves"} & set(what):
        sh(["cargo", "build", "-p", "twip-tools", "--profile", "bench"], src, env)
    if "geometry" in what:
        sh([tools, "dump", out / "geometry", FIXTURES], src, env)
    if "saves" in what:
        # Edits that fail are part of the output: both builds must fail them the same way.
        sh([tools, "roundtrip", "--edits", "all", "--keep", out / "saves", FIXTURES], src, env, check=False, out=out / "roundtrip.txt")
    tests = [TESTS[w] for w in what if w in TESTS]
    if tests:
        env.update(TWIP_FIDELITY_OUT=str(out / "fidelity"), TWIP_SCENARIO_OUT=str(out / "scenarios"), TWIP_REUSE_OUT=str(out / "reuse.txt"))
        sh(["cargo", "nextest", "run", "-p", "twip", "--lib", "--no-fail-fast", *tests], src, env, check=False)


def differences(a, b):
    """Files that differ or exist on one side only, relative to the two folders (or the two files)."""
    if a.is_file() or b.is_file():
        return [] if a.is_file() and b.is_file() and filecmp.cmp(a, b, shallow=False) else [a.name]
    out = []
    names = {p.relative_to(a) for p in a.rglob("*") if p.is_file()} | {p.relative_to(b) for p in b.rglob("*") if p.is_file()}
    for n in sorted(names):
        x, y = a / n, b / n
        if not (x.is_file() and y.is_file() and filecmp.cmp(x, y, shallow=False)):
            out.append(str(n))
    return out


def main():
    args = sys.argv[1:]
    if not args or args[0].startswith("-"):
        sys.exit(__doc__)
    rev = args[0]
    what = args[args.index("--only") + 1].split(",") if "--only" in args else ALL
    for side in ("base", "head"):
        sh(["rm", "-rf", str(WORK / side)], REPO)
        (WORK / side).mkdir(parents=True)
    produce("base", worktree(rev), what)
    produce("head", REPO, what)
    failed = False
    paths = {"geometry": "geometry", "saves": "saves", "fidelity": "fidelity", "scenarios": "scenarios", "reuse": "reuse.txt"}
    for w in what:
        a, b = WORK / "base" / paths[w], WORK / "head" / paths[w]
        if not a.exists():
            print(f"{w}: the base produces nothing to compare; skipped")
            continue
        if not b.exists():
            print(f"{w}: the head produced nothing")
            failed = True
            continue
        diff = differences(a, b)
        if w == "saves" and not differences(WORK / "base" / "roundtrip.txt", WORK / "head" / "roundtrip.txt") == []:
            diff.append("roundtrip.txt")
        print(f"{w}: {'identical' if not diff else f'{len(diff)} differ'}")
        for n in diff[:40]:
            print(f"  {n}")
        failed |= bool(diff)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
