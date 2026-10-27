"""Build and execute the private in-process language comparison."""

import os
from pathlib import Path
import shutil
import subprocess
import sys


def main():
    fixture = Path(__file__).resolve().parent
    root = fixture.parents[2]
    artifacts = root / "target" / "behavior-language-trial" / "managed"
    environment = {
        **os.environ,
        "CARGO_BUILD_JOBS": "1",
        "CARGO_TARGET_DIR": str(root / "target"),
        "DOTNET_CLI_TELEMETRY_OPTOUT": "1",
        "DOTNET_NOLOGO": "1",
    }
    executable = shutil.which("dotnet")
    if executable is None:
        raise RuntimeError("The comparison requires the .NET 10 SDK")
    hosts = Path(executable).resolve().parent / "host" / "fxr"
    versions = [
        path for path in hosts.iterdir()
        if len(path.name.split(".")) == 3 and all(part.isdigit() for part in path.name.split("."))
    ]
    latest = max(versions, key=lambda path: tuple(int(part) for part in path.name.split(".")))
    library = "hostfxr.dll" if os.name == "nt" else (
        "libhostfxr.dylib" if sys.platform == "darwin" else "libhostfxr.so"
    )
    environment["NARA_BEHAVIOR_HOSTFXR"] = str(latest / library)
    print(f"Using hostfxr {latest.name} ({library})", flush=True)
    subprocess.run(
        [
            "dotnet", "build", str(fixture / "managed" / "BehaviorTrial.csproj"),
            "--configuration", "Debug", "--artifacts-path", str(artifacts),
            "--disable-build-servers", "-m:1", "-p:UseSharedCompilation=false",
        ],
        cwd=root, env=environment, check=True,
    )
    environment["NARA_BEHAVIOR_MANAGED_DIR"] = str(artifacts / "bin" / "BehaviorTrial" / "debug")
    subprocess.run(
        [
            "cargo", "+1.95.0", "nextest", "run", "--manifest-path", str(fixture / "Cargo.toml"),
            "--locked", "--test", "languages", "--test-threads=1",
            "--status-level", "fail", "--final-status-level", "fail",
        ],
        cwd=root, env=environment, check=True,
    )


if __name__ == "__main__":
    main()
