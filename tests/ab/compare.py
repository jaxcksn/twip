"""Base against head: what two builds of the engine produce from the same inputs, compared byte for byte.

    geometry   every fixture's laid-out pages (`twip-tools dump`), glyph positions included
    saves      every fixture saved unedited and after each edit family (`twip-tools roundtrip --edits all --keep`)
    fidelity   the round-trip gate's saved packages (`TWIP_FIDELITY_OUT`)
    scenarios  what each editor scenario saves (`TWIP_SCENARIO_OUT`)
    reuse      layout reuse counts over a fixed edit script (`TWIP_REUSE_OUT`)
    alloc      allocations and model heap per load and per keystroke (`TWIP_ALLOC_OUT`); printed, not a gate
    stripped   what saving with every source anchor dropped loses (`TWIP_STRIPPED_OUT`); printed, not a gate

Saved packages are compared byte for byte; a package that differs is compared again part by part with the two
things no build controls masked: wall-clock times (revision, comment and property dates), content controls'
process-local ids (tests on several threads allocate them in any order), protection's random salt and the hash
made with it, and the order of the parts (the writer writes some in hash-map order). The report says how many
matched only that way.

Both builds read the head's fixtures. The base is built in its own worktree (target/ab/base) with its own target
folder, so the two builds never share one. A comparison the base cannot produce (its tests have no such hook) is
reported and skipped.

Run: python3 -I tests/ab/compare.py <base revision> [--only geometry,saves,...] [--compare-only]
(`--compare-only` compares what target/ab already holds, without building or running anything.)
Exits 1 when any output differs.
"""

import filecmp
import io
import os
import re
import subprocess
import sys
import zipfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
WORK = REPO / "target" / "ab"
FIXTURES = REPO / "crates" / "twip" / "tests" / "fixtures"
ALL = ["geometry", "saves", "fidelity", "scenarios", "reuse", "alloc", "stripped"]
TESTS = {
    "fidelity": "tests::fidelity",
    "scenarios": "editing::tests::scenarios",
    "reuse": "editing::tests::reuse",
    "alloc": "tests::allocations::allocations_per_load_and_per_keystroke",
    "stripped": "editing::tests::stripped",
}
REPORTS = {"alloc", "stripped"}


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
        env.update(
            TWIP_FIDELITY_OUT=str(out / "fidelity"),
            TWIP_SCENARIO_OUT=str(out / "scenarios"),
            TWIP_REUSE_OUT=str(out / "reuse.txt"),
            TWIP_ALLOC_OUT=str(out / "alloc.txt"),
            TWIP_STRIPPED_OUT=str(out / "stripped.txt"),
        )
        sh(["cargo", "nextest", "run", "-p", "twip", "--lib", "--no-fail-fast", *tests], src, env, check=False)


CLOCK = re.compile(rb"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ")
SALTED = re.compile(rb'((?:\w+:)?(?:hashValue|saltValue)=")[^"]*')
CONTROL_ID = re.compile(rb"(<(?:\w+:)?sdtPr>(?:(?!</(?:\w+:)?sdtPr>).)*?<(?:\w+:)?id (?:\w+:)?val=\")-?\d+", re.S)
MASKED = [0]


def same_package(x, y):
    """Whether two packages hold the same parts once clock times and control ids are masked."""
    try:
        a, b = zipfile.ZipFile(io.BytesIO(x.read_bytes())), zipfile.ZipFile(io.BytesIO(y.read_bytes()))
    except zipfile.BadZipFile:
        return False
    if sorted(a.namelist()) != sorted(b.namelist()):
        return False
    mask = lambda d: SALTED.sub(rb"\g<1>", CONTROL_ID.sub(rb"\g<1>0", CLOCK.sub(b"T", d)))
    return all(mask(a.read(n)) == mask(b.read(n)) for n in a.namelist())


def same(x, y):
    if filecmp.cmp(x, y, shallow=False):
        return True
    if x.suffix == ".docx" and same_package(x, y):
        MASKED[0] += 1
        return True
    return False


def differences(a, b):
    """Files that differ or exist on one side only, relative to the two folders (or the two files)."""
    if a.is_file() or b.is_file():
        return [] if a.is_file() and b.is_file() and same(a, b) else [a.name]
    out = []
    names = {p.relative_to(a) for p in a.rglob("*") if p.is_file()} | {p.relative_to(b) for p in b.rglob("*") if p.is_file()}
    for n in sorted(names):
        x, y = a / n, b / n
        if not (x.is_file() and y.is_file() and same(x, y)):
            out.append(str(n))
    return out


def report(what):
    """Compares what target/ab/base and target/ab/head hold; true when any gated output differs."""
    failed = False
    paths = {"geometry": "geometry", "saves": "saves", "fidelity": "fidelity", "scenarios": "scenarios", "reuse": "reuse.txt", "alloc": "alloc.txt", "stripped": "stripped.txt"}
    for w in what:
        a, b = WORK / "base" / paths[w], WORK / "head" / paths[w]
        if not a.exists():
            print(f"{w}: the base produces nothing to compare; skipped")
            continue
        if not b.exists():
            print(f"{w}: the head produced nothing")
            failed = True
            continue
        if w in REPORTS:
            print(f"{w} (base, then head):")
            for side in (a, b):
                print("".join(f"  {line}" for line in side.read_text(encoding="utf-8").splitlines(keepends=True)[-12:]))
            continue
        MASKED[0] = 0
        diff = differences(a, b)
        if w == "saves" and not differences(WORK / "base" / "roundtrip.txt", WORK / "head" / "roundtrip.txt") == []:
            diff.append("roundtrip.txt")
        masked = f" ({MASKED[0]} only with the uncontrolled values masked)" if MASKED[0] else ""
        print(f"{w}: {'identical' if not diff else f'{len(diff)} differ'}{masked}")
        for n in diff[:40]:
            print(f"  {n}")
        failed |= bool(diff)
    return failed


def main():
    args = sys.argv[1:]
    if not args or args[0].startswith("-"):
        sys.exit(__doc__)
    rev = args[0]
    what = args[args.index("--only") + 1].split(",") if "--only" in args else ALL
    if "--compare-only" in args:
        sys.exit(1 if report(what) else 0)
    for side in ("base", "head"):
        sh(["rm", "-rf", str(WORK / side)], REPO)
        (WORK / side).mkdir(parents=True)
    produce("base", worktree(rev), what)
    produce("head", REPO, what)
    sys.exit(1 if report(what) else 0)



if __name__ == "__main__":
    main()
