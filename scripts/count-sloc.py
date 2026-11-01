"""Partition first-party SLoC, including inline Rust tests. Requires tokei and cargo.

Usage: python scripts/count-sloc.py [--revision COMMIT] [--output report.json]
Only tracked/nonignored source under crates, metatorio-app and scripts is counted.
Historical snapshots use exactly the same classifier as the working tree.
"""
import argparse
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
ROOTS = ["crates", "metatorio-app", "scripts"]
TYPES = "Rust,TypeScript,JavaScript,Svelte,CSS,HTML,Python,PowerShell"
EXTENSIONS = {".rs", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".svelte", ".css", ".html", ".py", ".ps1"}


def run(*args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, stdout=subprocess.PIPE, **kwargs).stdout


def sources(revision):
    if revision:
        paths = run("git", "ls-tree", "-r", "--name-only", revision, "--", *ROOTS)
    else:
        paths = run("git", "ls-files", "--cached", "--others", "--exclude-standard", "--", *ROOTS)
    names = sorted({p for p in paths.decode().splitlines() if Path(p).suffix in EXTENSIONS})
    if not revision:
        return {name: (ROOT / name).read_bytes() for name in names if (ROOT / name).is_file()}
    requests = "".join(f"{revision}:{name}\n" for name in names).encode()
    stream = io.BytesIO(run("git", "cat-file", "--batch", input=requests))
    result = {}
    for name in names:
        header = stream.readline().split()
        if len(header) != 3 or header[1] != b"blob":
            raise ValueError(f"Cannot read {revision}:{name}: {header}")
        result[name] = stream.read(int(header[2]))
        assert stream.read(1) == b"\n"
    return result


def category(name, source):
    path = Path(name)
    if "tests" in path.parts or "benches" in path.parts or path.stem.endswith("_tests") or path.stem.startswith("test_"):
        return "tests"
    if "examples" in path.parts:
        return "examples"
    if path.parts[0] == "scripts":
        return "tooling"
    header = source.decode("utf-8-sig").splitlines()[:5]
    if any(line.lstrip().startswith(("// @generated", "// This file has been generated")) for line in header):
        return "generated"
    return "production"


def count(directory):
    if not any(directory.rglob("*.*")):
        return 0
    return json.loads(run("tokei", str(directory), "--types", TYPES, "-o", "json"))["Total"]["code"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--revision")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    files = sources(args.revision)
    manifest = ROOT / "scripts/sloc-rust/Cargo.toml"
    subprocess.run(["cargo", "build", "--offline", "--locked", "--quiet", "--manifest-path", str(manifest)], cwd=ROOT, check=True)
    executable = manifest.parent / "target/debug" / ("metatorio-sloc-rust.exe" if os.name == "nt" else "metatorio-sloc-rust")
    with tempfile.TemporaryDirectory(prefix="metatorio-sloc-") as temporary:
        base = Path(temporary)
        original = base / "original"
        groups = {group: base / group for group in ["production", "tests", "generated", "examples", "tooling"]}
        for directory in [original, *groups.values()]:
            directory.mkdir()
        for name, source in files.items():
            path = original / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(source)
        rust = [original / name for name in files if name.endswith(".rs")]
        parsed = run(str(executable), input="".join(f"{path}\n" for path in rust).encode())
        metadata = [json.loads(line) for line in parsed.splitlines()]
        ranges = {Path(entry["path"]): entry["ranges"] for entry in metadata}
        test_modules = [Path(module) for entry in metadata for module in entry["modules"]]
        classifications = {}
        for name, source in files.items():
            path = original / name
            group = category(name, source)
            if any(path == module or module in path.parents for module in test_modules):
                group = "tests"
            classifications[name] = group
            destination = groups[group] / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            if group not in {"tests", "generated"} and ranges.get(path):
                lines = source.decode("utf-8-sig").splitlines(keepends=True)
                for first, last in ranges[path]:
                    lines[first - 1:last] = ["\n"] * (last - first + 1)
                destination.write_text("".join(lines), encoding="utf-8", newline="")
            else:
                destination.write_bytes(source)
        totals = {group: count(directory) for group, directory in groups.items()}
        total = count(original)
        # The difference is inline test code removed from production files.
        inline = total - sum(totals.values())
        if inline < 0:
            raise ValueError("Partition counts exceeded original SLoC")
        totals["tests"] += inline
        report = {
            "revision": args.revision or "working-tree",
            "tokei": run("tokei", "--version").decode().strip(),
            "sloc": {**totals, "total": total, "inline_rust_tests": inline},
            "files": classifications,
        }
        output = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
        if args.output:
            args.output.write_text(output, encoding="utf-8")
        print(json.dumps(report["sloc"], ensure_ascii=False))


if __name__ == "__main__":
    main()
