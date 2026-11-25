"""Check the v2 dependency boundaries and keep retired patterns out of new Rust code."""

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


def check_dependencies(package, forbidden, *features):
    graph = packages("-p", package, *features) - {package}
    unexpected = sorted(name for name in graph if forbidden.match(name))
    if unexpected:
        raise SystemExit(f"{package} includes forbidden packages: {', '.join(unexpected)}")


def check_wgpu():
    for path in (
        "Cargo.lock",
        "reference-game/Cargo.lock",
        "samples/collect-game/Cargo.lock",
        "samples/two-level-game/Cargo.lock",
        "examples/shared-viewport/Cargo.lock",
    ):
        with Path(path).open("rb") as locked:
            wgpu = [entry for entry in tomllib.load(locked)["package"] if entry["name"] == "wgpu"]
        if len(wgpu) != 1:
            raise SystemExit(f"{path} resolves {len(wgpu)} wgpu package identities")
        yield wgpu[0]["version"], wgpu[0]["source"]


def check_rust_sources():
    prohibited = re.compile(
        r"\bpub\s+mod\s+__private\b|\b__RuntimeDriverPort\b|"
        r"\b(?:std::)?process::abort\s*\(|\b(?:std::)?mem::forget\s*\("
    )
    files = subprocess.check_output(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--",
         "src", "crates", "examples", "reference-game", "samples", "starters", "tests"],
        encoding="utf-8",
    ).split("\0")
    for path in sorted(set(files)):
        if not path.endswith(".rs") or not Path(path).is_file():
            continue
        for number, line in enumerate(Path(path).read_text(encoding="utf-8").splitlines(), 1):
            if not line.lstrip().startswith("//") and prohibited.search(line):
                raise SystemExit(f"{path}:{number} uses a retired pattern: {line.strip()}")


if __name__ == "__main__":
    ui = r"(?:open[-_]gpui|gpui)(?:[-_]|$)"
    editor = rf"{ui}|nara[-_](?:editor|tooling)(?:[-_]|$)|dear[-_]imgui"
    check_dependencies("nara", re.compile(rf"{editor}|ctrlc$"), "--all-features")
    check_dependencies(
        "nara", re.compile(rf"{editor}|nara_project$|bevy_reflect$"),
        "--no-default-features", "--features",
        "physics-2d,audio-device,presentation-2d,presentation-ui,desktop-winit,render-wgpu",
    )
    check_dependencies("nara_project", re.compile(rf"{editor}|^(?:wgpu|winit)$"), "--all-features")
    check_dependencies("nara_editor_core", re.compile(ui), "--all-features")
    if len(set(check_wgpu())) != 1:
        raise SystemExit("The engine and game workspaces resolve different wgpu package identities")
    check_rust_sources()
    first_game = Path("examples/first_game.rs")
    lines = len(first_game.read_text(encoding="utf-8").splitlines())
    if lines > 100:
        raise SystemExit(f"{first_game} has {lines} lines; the first-game limit is 100")
    print("Dependency and source boundaries passed.")
