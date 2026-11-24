"""Check the v2 dependency boundaries and keep retired patterns out of new Rust code."""

import os
from pathlib import Path
import re
import subprocess
import tomllib


def packages(*arguments):
    tree = subprocess.check_output(
        ["cargo", "tree", "--locked", "--color=never", "--edges", "normal,build",
         "--prefix", "none", *arguments],
        text=True,
    )
    return {line.split()[0] for line in tree.splitlines() if line.strip()}


def check_dependencies(package, forbidden):
    graph = packages("-p", package, "--all-features") - {package}
    unexpected = sorted(name for name in graph if forbidden.match(name))
    if unexpected:
        raise SystemExit(f"{package} includes forbidden packages: {', '.join(unexpected)}")


def check_wgpu():
    for path in (
        "Cargo.lock",
        "reference-game/Cargo.lock",
        "samples/collect-game/Cargo.lock",
        "samples/two-level-game/Cargo.lock",
    ):
        with Path(path).open("rb") as locked:
            wgpu = [entry for entry in tomllib.load(locked)["package"] if entry["name"] == "wgpu"]
        if len(wgpu) != 1:
            raise SystemExit(f"{path} resolves {len(wgpu)} wgpu package identities")
        yield wgpu[0]["version"], wgpu[0]["source"]


def check_rust_changes(base):
    prohibited = re.compile(r"\bpub\s+mod\s+__private\b|\b(?:std::)?process::abort\s*\(|\b(?:std::)?mem::forget\s*\(")
    diff = subprocess.check_output(
        ["git", "diff", "--no-ext-diff", "--no-renames", "--unified=0", base, "--",
         "src", "crates", ":(glob)**/src/**/*.rs", ":(exclude,glob)**/tests/**",
         ":(exclude,glob)**/tests.rs", ":(exclude,glob)**/*_tests.rs"],
        text=True,
        encoding="utf-8",
    )
    path = ""
    for line in diff.splitlines():
        if line.startswith("+++ b/"):
            path = line[6:]
        elif line.startswith("+++ "):
            path = ""
        elif path.endswith(".rs") and line.startswith("+"):
            code = line[1:].strip()
            if not code.startswith("//") and prohibited.search(code):
                raise SystemExit(f"{path} adds a retired pattern: {code}")


if __name__ == "__main__":
    ui = r"(?:open[-_]gpui|gpui)(?:[-_]|$)"
    check_dependencies("nara", re.compile(rf"{ui}|nara[-_](?:editor|tooling)(?:[-_]|$)|dear[-_]imgui"))
    check_dependencies("nara_editor_core", re.compile(ui))
    if len(set(check_wgpu())) != 1:
        raise SystemExit("The engine and game workspaces resolve different wgpu package identities")
    base = os.environ.get("BASE", "")
    if not base or set(base) == {"0"}:
        base = "HEAD^"
    check_rust_changes(base)
    print("Dependency and source boundaries passed.")
