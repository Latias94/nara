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
    expected = (
        "30.0.0",
        "git+https://github.com/gfx-rs/wgpu?rev=cddb2f5d80dc0009b2ef6767635140cacf90a007"
        "#cddb2f5d80dc0009b2ef6767635140cacf90a007",
    )
    for path in (
        "Cargo.lock",
        "crates/nara_editor/Cargo.lock",
        "reference-game/Cargo.lock",
        "examples/shared-viewport/Cargo.lock",
    ):
        with Path(path).open("rb") as locked:
            wgpu = [entry for entry in tomllib.load(locked)["package"] if entry["name"] == "wgpu"]
        if len(wgpu) != 1:
            raise SystemExit(f"{path} resolves {len(wgpu)} wgpu package identities")
        found = wgpu[0]["version"], wgpu[0].get("source")
        if found != expected:
            raise SystemExit(f"{path} resolves an unexpected wgpu package: {found}")


def check_rust_sources():
    prohibited = re.compile(
        r"\bpub\s+mod\s+__private\b|\b__RuntimeDriverPort\b|"
        r"\b(?:std::)?process::abort\s*\(|\b(?:std::)?mem::forget\s*\("
    )
    files = subprocess.check_output(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--",
         "src", "crates", "examples", "reference-game", "tests"],
        encoding="utf-8",
    ).split("\0")
    for path in sorted(set(files)):
        if not path.endswith(".rs") or not Path(path).is_file():
            continue
        for number, line in enumerate(Path(path).read_text(encoding="utf-8").splitlines(), 1):
            if not line.lstrip().startswith("//") and prohibited.search(line):
                raise SystemExit(f"{path}:{number} uses a retired pattern: {line.strip()}")


def check_source_size():
    def rust_lines(directory):
        return sum(
            len(path.read_text(encoding="utf-8").splitlines())
            for path in directory.rglob("*.rs")
        )

    facade = rust_lines(Path("src"))
    if facade > 1_500:
        raise SystemExit(f"Root src has {facade:,} Rust lines; the limit is 1,500")
    render = rust_lines(Path("crates/nara_render"))
    if render > 28_000:
        raise SystemExit(f"nara_render has {render:,} Rust lines; the limit is 28,000")
    for manifest in sorted(Path("crates").glob("*/Cargo.toml")):
        lines = rust_lines(manifest.parent / "src")
        if lines > 30_000:
            print(f"Warning: {manifest.parent.name}/src has {lines:,} Rust lines; review its scope")


if __name__ == "__main__":
    ui = r"(?:open[-_]gpui|gpui)(?:[-_]|$)"
    editor = rf"{ui}|nara[-_](?:editor|tooling)(?:[-_]|$)|dear[-_]imgui"
    shipping = re.compile(rf"{editor}|nara_project$|bevy_reflect$")
    check_dependencies("nara", re.compile(rf"{editor}|ctrlc$"), "--all-features")
    check_dependencies(
        "nara", shipping,
        "--no-default-features", "--features",
        "physics-2d,audio-device,presentation-2d,presentation-ui,desktop-winit,render-wgpu",
    )
    check_dependencies(
        "nara-reference-game", shipping, "--manifest-path", "reference-game/Cargo.toml",
        "--features", "desktop",
    )
    check_dependencies(
        "nara-reference-game", re.compile(rf"{shipping.pattern}|^(?:notify|png|ron)$"),
        "--manifest-path", "reference-game/Cargo.toml", "--no-default-features", "--features", "desktop",
    )
    check_dependencies("nara_project", re.compile(rf"{editor}|^(?:wgpu|winit)$"), "--all-features")
    check_dependencies(
        "nara_editor_core",
        re.compile(rf"{editor}|(?:wgpu|winit|dockspace)(?:[-_]|$)"),
        "--all-features",
    )
    editor_packages = re.compile(editor)
    for manifest in sorted(Path("crates").glob("nara_*/Cargo.toml")):
        with manifest.open("rb") as stream:
            package = tomllib.load(stream)["package"]["name"]
        if not editor_packages.match(package) and package != "nara_project":
            check_dependencies(package, shipping, "--all-features")
    check_wgpu()
    check_rust_sources()
    check_source_size()
    first_game = Path("examples/first_game.rs")
    lines = len(first_game.read_text(encoding="utf-8").splitlines())
    if lines > 100:
        raise SystemExit(f"{first_game} has {lines} lines; the first-game limit is 100")
    print("Dependency and source boundaries passed.")
