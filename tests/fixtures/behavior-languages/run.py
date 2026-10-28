"""Build gameplay modules and a standalone source tool, then exercise their consumers."""

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(
        description="Build and qualify the finite behavior-language and source-tool consumers."
    )
    parser.add_argument(
        "--native-editor",
        action="store_true",
        help="Run the Windows DX12 native Editor source-tool qualification after the default tests.",
    )
    options = parser.parse_args()
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
    dotnet = Path(executable).resolve()
    environment["NARA_SOURCE_TOOL_DOTNET"] = str(dotnet)
    hosts = dotnet.parent / "host" / "fxr"
    versions = [
        path for path in hosts.iterdir()
        if len(path.name.split(".")) == 3 and all(part.isdigit() for part in path.name.split("."))
    ]
    latest = max(versions, key=lambda path: tuple(int(part) for part in path.name.split(".")))
    library = "hostfxr.dll" if os.name == "nt" else (
        "libhostfxr.dylib" if sys.platform == "darwin" else "libhostfxr.so"
    )
    environment["NARA_BEHAVIOR_HOSTFXR"] = str(latest / library)
    framework_versions = [
        path.name for path in (dotnet.parent / "shared" / "Microsoft.NETCore.App").iterdir()
        if len(path.name.split(".")) == 3
        and all(part.isdigit() for part in path.name.split("."))
        and path.name.startswith("10.0.")
    ]
    environment["NARA_SOURCE_TOOL_FRAMEWORK"] = max(
        framework_versions, key=lambda version: tuple(int(part) for part in version.split("."))
    )
    print(f"Using hostfxr {latest.name} ({library})", flush=True)

    def build(project, destination, **properties):
        command = [
            str(dotnet), "build", str(project),
            "--configuration", "Debug", "--artifacts-path", str(destination),
            "--disable-build-servers", "-m:1", "-p:UseSharedCompilation=false",
        ]
        command.extend(f"-p:{name}={value}" for name, value in properties.items())
        subprocess.run(command, cwd=root, env=environment, check=True)

    bootstrap = root / "target" / "source-tool-host"
    build(root / "tools" / "source-tool-host" / "Nara.SourceToolHost.csproj", bootstrap)
    environment["NARA_SOURCE_TOOL_BOOTSTRAP_DIR"] = str(
        bootstrap / "bin" / "Nara.SourceToolHost" / "debug"
    )
    build(fixture / "managed" / "BehaviorTrial.csproj", artifacts)
    environment["NARA_BEHAVIOR_MANAGED_DIR"] = str(artifacts / "bin" / "BehaviorTrial" / "debug")
    module_project = fixture / "authored" / "BehaviorModule.csproj"
    modules = artifacts.parent / "modules"
    for name, revision in [
        ("v1", "1"), ("invalid", "invalid"), ("missing", "missing"), ("live", "1"),
    ]:
        destination = modules / name
        build(module_project, destination, TrialRevision=revision)
        environment[f"NARA_BEHAVIOR_MODULE_{name.upper()}"] = str(
            destination / "bin" / "BehaviorModule" / "debug" / "Nara.AuthoredBehaviors.dll"
        )
    environment["NARA_BEHAVIOR_MODULE_PROJECT"] = str(module_project)
    environment["NARA_BEHAVIOR_LIVE_ARTIFACTS"] = str(modules / "live")
    tool_project = fixture / "tools" / "EnemyStatsTool.csproj"
    for name, revision in [("v1", "1"), ("v2", "2"), ("invalid", "invalid")]:
        destination = artifacts.parent / "tools" / name
        build(tool_project, destination, ToolRevision=revision)
        environment[f"NARA_SOURCE_TOOL_{name.upper()}_DIR"] = str(
            destination / "bin" / "EnemyStatsTool" / "debug"
        )
    catalogs = artifacts.parent / "creation-catalogs"
    subprocess.run(
        [
            "cargo", "+1.95.0", "run", "--manifest-path", str(fixture / "Cargo.toml"),
            "--locked", "--example", "creation_catalogs", "--",
            str(Path(environment["NARA_SOURCE_TOOL_V1_DIR"]) / "enemy-stats.catalog.json"),
            str(catalogs),
        ], cwd=root, env=environment, check=True,
    )
    for name in ["creation_v2", "creation_wrong_predecessor", "creation_unsupported",
                 "creation_invalid"]:
        metadata = catalogs / ("creation_v2" if name == "creation_invalid" else name)
        destination = artifacts.parent / "tools" / name
        build(tool_project, destination,
              ToolRevision="invalid" if name == "creation_invalid" else "2",
              ToolCatalog=metadata / "enemy-stats.catalog.json")
        published = destination / "bin" / "EnemyStatsTool" / "debug"
        shutil.copyfile(metadata / "enemy-stats.predecessor.catalog.json",
                        published / "enemy-stats.predecessor.catalog.json")
        environment[f"NARA_SOURCE_TOOL_{name.upper()}_DIR"] = str(published)
    for name, revision, mode in [
        ("v1", "1", "sync"), ("v2", "2", "sync"),
        ("async", "1", "async"), ("sta", "1", "sta"),
    ]:
        destination = artifacts.parent / "probes" / name
        build(fixture / "managed-tool-probes" / "Entry" / "Entry.csproj", destination,
              DependencyRevision=revision, ProbeMode=mode)
        environment[f"NARA_MANAGED_PROBE_{name.upper()}_DIR"] = str(
            destination / "bin" / "Entry" / "debug"
        )
    subprocess.run(
        [
            "cargo", "+1.95.0", "nextest", "run", "--manifest-path", str(fixture / "Cargo.toml"),
            "--locked", "--test", "languages", "--test-threads=1", "--no-fail-fast",
            "--status-level", "fail", "--final-status-level", "fail", "--success-output", "final",
        ],
        cwd=root, env=environment, check=True,
    )
    if os.name == "nt":
        subprocess.run(
            [
                "cargo", "+1.95.0", "nextest", "run", "-p", "nara", "--all-features",
                "--lib", "--locked", "--run-ignored", "only", "-E",
                "test(managed_child_cancellation_retains_approved_custody)",
                "--test-threads=1", "--status-level", "fail", "--final-status-level", "fail",
            ], cwd=root, env=environment, check=True,
        )
        subprocess.run(
            [
                "cargo", "+1.95.0", "nextest", "run", "-p", "nara-editor", "--lib", "--locked",
                "--run-ignored", "only", "-E",
                "test(csharp_source_tool_application_preserves_review_and_exact_commit_result) | "
                "test(csharp_source_tool_creation_metadata_evolution_preserves_existing_data_and_history)",
                "--test-threads=1", "--status-level", "fail", "--final-status-level", "fail",
            ],
            cwd=root, env=environment, check=True,
        )
        if options.native_editor:
            native_environment = {**environment, "WGPU_BACKEND": "dx12"}
            subprocess.run(
                [
                    "cargo", "+1.95.0", "nextest", "run", "-p", "nara-editor",
                    "--all-features", "--lib", "--locked", "--run-ignored", "only", "-E",
                    "test(native_source_tool_inline_property_and_panel_share_review_history_and_persistence)",
                    "--test-threads=1", "--status-level", "fail", "--final-status-level", "fail",
                ],
                cwd=root, env=native_environment, check=True,
            )


if __name__ == "__main__":
    main()
