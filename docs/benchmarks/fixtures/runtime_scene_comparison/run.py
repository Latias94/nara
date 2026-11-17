"""Run prebuilt CPU consumers serially; retain samples without building any engine."""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import subprocess
import time
from datetime import datetime, timezone


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--nara", type=Path, required=True)
    parser.add_argument("--bevy", type=Path, required=True)
    parser.add_argument("--godot", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    executables = {name: getattr(args, name).resolve(strict=True) for name in ("nara", "bevy", "godot")}
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    fixture = Path(__file__).resolve().parent
    project = output / "godot-project"
    project.mkdir()
    for name in ("project.godot", "main.gd"):
        shutil.copyfile(fixture / "godot" / name, project / name)
    records = []
    metadata = []
    processes = []
    started = datetime.now(timezone.utc).isoformat()
    orders = (("nara", "bevy", "godot"), ("godot", "bevy", "nara"), ("bevy", "nara", "godot"))
    for round_index, order in enumerate(orders, 1):
        for engine in order:
            directory = output / f"round-{round_index}-{engine}"
            directory.mkdir()
            command = [str(executables[engine]), str(directory)]
            if engine == "godot":
                command = [str(executables[engine]), "--headless", "--path", str(project), "--script", "main.gd", "--", str(directory)]
            before = time.perf_counter_ns()
            result = subprocess.run(command, capture_output=True, encoding="utf-8", errors="strict", timeout=180)
            wall_ns = time.perf_counter_ns() - before
            (output / f"round-{round_index}-{engine}.stdout").write_text(result.stdout, encoding="utf-8", newline="\n")
            (output / f"round-{round_index}-{engine}.stderr").write_text(result.stderr, encoding="utf-8", newline="\n")
            if result.returncode:
                raise RuntimeError(f"{engine} round {round_index} exit {result.returncode}; inspect retained stdout/stderr")
            current = []
            for line in result.stdout.splitlines():
                if not line.startswith("{"):
                    continue
                record = json.loads(line)
                record["round"] = round_index
                if record.get("kind") == "metadata":
                    metadata.append(record)
                    continue
                if not record.get("state_verified"):
                    raise RuntimeError(f"{engine}: unverified state")
                count = record["rows"]
                side = math.isqrt(count)
                expected = sum(i % side + i // side + i % 64 + i % 4 for i in range(count))
                if record["checksum"] != expected or record["samples"] != 9 or record["warmup"] != 3:
                    raise RuntimeError(f"{engine}: inconsistent workload or samples")
                for key, values in record.items():
                    if key.endswith("_ns") and isinstance(values, list):
                        if len(values) != 9 or any(not isinstance(value, int) or value < 0 for value in values):
                            raise RuntimeError(f"{engine}: invalid {key} samples")
                record["source_sha256"] = None
                path = record.get("source_file")
                if path:
                    record["source_sha256"] = digest(Path(path))
                current.append(record)
            expected_records = 6 if engine == "bevy" else 9
            if len(current) != expected_records:
                raise RuntimeError(f"{engine}: expected {expected_records} records, got {len(current)}")
            records.extend(current)
            processes.append({"engine": engine, "round": round_index, "wall_ns": wall_ns, "exit": result.returncode})
            print(f"round={round_index} engine={engine} cases={len(current)} wall_ms={wall_ns / 1e6:.3f}", flush=True)
    receipt = {
        "format": "nara-local-runtime-module-comparison/v1",
        "started_utc": started,
        "finished_utc": datetime.now(timezone.utc).isoformat(),
        "host": {"platform": platform.platform(), "processor": os.environ.get("PROCESSOR_IDENTIFIER", platform.processor()), "logical_cpus": os.cpu_count()},
        "order": orders,
        "executables": {name: {"path": str(path), "sha256": digest(path)} for name, path in executables.items()},
        "fixture_sha256": {
            str(path.relative_to(fixture)).replace("\\", "/"): digest(path)
            for path in sorted(fixture.rglob("*"))
            if path.is_file() and not any(part in (".godot", "target", "__pycache__") for part in path.parts)
        },
        "limits": ["warm OS file cache; independent process rounds", "no cold storage, exclusive CPU attribution, GPU, frame timing or memory measurements", "Godot decode includes an extra file read; do not subtract separate warm-read samples", "no unrelated Cargo or benchmark launched by this runner; external system load is uncontrolled"],
        "processes": processes,
        "consumer_metadata": metadata,
        "records": records,
    }
    (output / "observations.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(output / "observations.json", flush=True)


if __name__ == "__main__":
    main()
