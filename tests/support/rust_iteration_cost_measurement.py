"""Measure this checkout's Collect Game rebuild/restart path, without editing the sample.

Run serially with other Cargo work. This decision-local experiment measures successful
headless process completion, including startup, semantic execution, cleanup and exit.
It does not measure a presented frame, hot patching, or an internal simulation interval.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import tempfile
import time


def replace_once(source, before, after):
    if source.count(before) != 1:
        raise RuntimeError(f"expected exactly one source match: {before!r}")
    return source.replace(before, after, 1)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def checked_output(command, *, cwd, env, timeout):
    result = subprocess.run(
        command, cwd=cwd, env=env, capture_output=True, timeout=timeout, check=False
    )
    if result.returncode:
        raise RuntimeError(
            f"command failed ({result.returncode}): {command!r}\n"
            + result.stdout.decode("utf-8", errors="replace")
            + result.stderr.decode("utf-8", errors="replace")
        )
    return result.stdout.decode("utf-8", errors="strict").strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--samples", type=int, choices=range(1, 11), default=5)
    parser.add_argument("--output", type=Path)
    arguments = parser.parse_args()
    checkout = Path(__file__).resolve().parents[2]
    temporary_parent = checkout / "target" / "tmp"
    temporary_parent.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="rust-iteration-", dir=temporary_parent))
    project = work / "collect-game"
    shutil.copytree(checkout / "samples" / "collect-game", project)
    print(f"Retained disposable project and logs: {work}", flush=True)

    coordinate = json.dumps(checkout.as_posix()).encode("utf-8")
    for manifest, old_path in [
        (project / "Cargo.toml", b'path = "../.."'),
        (project / "gameplay-plugin" / "Cargo.toml", b'path = "../../.."'),
    ]:
        manifest.write_bytes(replace_once(manifest.read_bytes(), old_path, b"path = " + coordinate))

    source_path = project / "gameplay-plugin" / "src" / "lib.rs"
    original = source_path.read_bytes()
    body_a = replace_once(
        original,
        b"direction * config.speed * time.delta()",
        b"direction * (config.speed * 1.0) * time.delta()",
    )
    body_b = replace_once(body_a, b"config.speed * 1.0", b"config.speed * 1.1")
    newline = b"\r\n" if b"\r\n" in original else b"\n"
    structural_b = replace_once(
        body_a,
        b"    held: [bool; 4],",
        b"    held: [bool; 4]," + newline + b"    iteration_speed_multiplier: f32,",
    )
    structural_b = replace_once(
        structural_b,
        b"app.insert_resource(GameState::default())?",
        b"app.insert_resource(GameState { iteration_speed_multiplier: 1.1, "
        b"..GameState::default() })?",
    )
    structural_b = replace_once(
        structural_b,
        b"config.speed * 1.0",
        b"config.speed * state.iteration_speed_multiplier",
    )
    for name, source in [("baseline", body_a), ("body", body_b), ("structural", structural_b)]:
        (work / f"{name}.rs").write_bytes(source)

    environment = os.environ.copy()
    environment.update(
        CARGO_BUILD_JOBS="1", CARGO_INCREMENTAL="1", CARGO_TARGET_DIR=str(checkout / "target")
    )
    for name in ["NARA_STARTER_OBSERVATION_ATTEMPT", "NARA_STARTER_OBSERVATION_FILE"]:
        environment.pop(name, None)
    build_command = [
        "cargo", "build", "--offline", "--locked", "--manifest-path",
        str(project / "Cargo.toml"), "--bin", "headless", "--message-format=json",
    ]
    report = {
        "scope": "decision-local Collect Game dev incremental rebuild and fresh headless process",
        "endpoint": "successful_headless_process_exit; semantic oracle checked after timing",
        "first_present_measured": False,
        "rustc": checked_output(["rustc", "-Vv"], cwd=project, env=environment, timeout=30),
        "cargo": checked_output(["cargo", "-V"], cwd=project, env=environment, timeout=30),
        "source_revision": checked_output(
            ["git", "rev-parse", "HEAD"], cwd=checkout, env=environment, timeout=30
        ),
        "initial_status": checked_output(
            ["git", "status", "--short"], cwd=checkout, env=environment, timeout=30
        ),
        "sample_source_sha256": digest(original),
        "sample_lock_sha256": digest((project / "Cargo.lock").read_bytes()),
        "build_command": build_command,
        "environment": {
            name: environment.get(name)
            for name in [
                "CARGO_BUILD_JOBS", "CARGO_INCREMENTAL", "CARGO_TARGET_DIR", "RUSTUP_TOOLCHAIN",
                "CARGO_BUILD_TARGET", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS",
                "CARGO_PROFILE_DEV_OPT_LEVEL", "CARGO_PROFILE_DEV_DEBUG",
                "CARGO_PROFILE_DEV_INCREMENTAL",
            ]
        },
        "samples_per_class": arguments.samples,
        "observations": [],
    }
    output = arguments.output.resolve() if arguments.output else work / "results.json"
    output.parent.mkdir(parents=True, exist_ok=True)

    def save_report():
        output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    def observe(kind, index, source, expected_x, *, warmup=False, edit=True):
        name = f"{kind}-{index:02}"
        write_started = time.perf_counter_ns()
        if edit:
            source_path.write_bytes(source)
        saved = time.perf_counter_ns()
        build_started = time.perf_counter_ns()
        build = subprocess.run(
            build_command, cwd=project, env=environment, capture_output=True,
            timeout=900 if warmup else 180, check=False,
        )
        built = time.perf_counter_ns()
        if build.returncode:
            (work / f"{name}.build.jsonl").write_bytes(build.stdout)
            (work / f"{name}.build.stderr.log").write_bytes(build.stderr)
            raise RuntimeError(f"{name}: Cargo failed; inspect retained logs")
        artifacts = [
            message["executable"]
            for line in build.stdout.splitlines()
            if (message := json.loads(line)).get("reason") == "compiler-artifact"
            and message["target"]["name"] == "headless"
            and message.get("executable")
        ]
        if len(artifacts) != 1:
            raise RuntimeError(f"{name}: expected one actual headless executable: {artifacts!r}")
        run_started = time.perf_counter_ns()
        run = subprocess.run(
            [artifacts[0]], cwd=project, env=environment, capture_output=True,
            timeout=30, check=False,
        )
        completed = time.perf_counter_ns()
        # Retain diagnostics after the endpoint; avoid timing unrelated log publication.
        (work / f"{name}.build.jsonl").write_bytes(build.stdout)
        (work / f"{name}.build.stderr.log").write_bytes(build.stderr)
        (work / f"{name}.headless.stdout.log").write_bytes(run.stdout)
        (work / f"{name}.headless.stderr.log").write_bytes(run.stderr)
        outcomes = re.findall(
            rb"^collect_headless: ok tick=62 collected=3 targets=3 won=true resets=0 "
            rb"player=\(([-0-9.]+),0\.0\)\r?$",
            run.stdout, flags=re.MULTILINE,
        )
        if run.returncode or len(outcomes) != 1 or float(outcomes[0]) != expected_x:
            raise RuntimeError(f"{name}: changed semantic outcome failed; inspect retained logs")
        if source_path.read_bytes() != source:
            raise RuntimeError(f"{name}: disposable source changed during the observation")
        observation = {
            "class": kind, "sample": index, "warmup": warmup, "source_changed": edit,
            "source_sha256": digest(source), "player_x": expected_x,
            "source_write_ns": saved - write_started if edit else None,
            "build_command_ns": built - build_started,
            "headless_process_ns": completed - run_started,
            "saved_source_to_headless_exit_ns": completed - saved,
            "log_prefix": name,
        }
        report["observations"].append(observation)
        save_report()
        print(
            f"{name}: x={expected_x:.1f}; build={(built - build_started) / 1e9:.3f}s; "
            f"headless={(completed - run_started) / 1e9:.3f}s; "
            f"total={(completed - saved) / 1e9:.3f}s; warmup={warmup}",
            flush=True,
        )

    save_report()
    observe("initial-warmup", 0, body_a, 108.0, warmup=True)
    for index in range(1, arguments.samples + 1):
        observe("no-edit", index, body_a, 108.0, edit=False)
    for index in range(1, arguments.samples + 1):
        changed = index % 2 == 1
        observe("body", index, body_b if changed else body_a, 108.8 if changed else 108.0)
    observe("structural-baseline-warmup", 0, body_a, 108.0, warmup=True)
    for index in range(1, arguments.samples + 1):
        changed = index % 2 == 1
        observe(
            "structural", index, structural_b if changed else body_a,
            108.8 if changed else 108.0,
        )
    report["summary_ns"] = {}
    for kind in ["no-edit", "body", "structural"]:
        values = [
            row["saved_source_to_headless_exit_ns"]
            for row in report["observations"] if row["class"] == kind
        ]
        report["summary_ns"][kind] = {
            "count": len(values), "median": statistics.median(values),
            "min": min(values), "max": max(values),
        }
    report["limits"] = [
        "Fixed order and small sample; no stable P95/tail or cross-platform claim.",
        "No-edit starts before Cargo; edited intervals start after a closed source write.",
        "Build includes Cargo resolution, frontend, codegen and link; no attribution within it.",
        "Headless process includes startup, semantic execution, cleanup, output and process exit.",
        "This uses current game-source dependency coordinates, not a general code-freshness proof.",
        "Retained disposable source and logs are not removed automatically.",
    ]
    save_report()
    print(f"Results: {output}", flush=True)


if __name__ == "__main__":
    main()
