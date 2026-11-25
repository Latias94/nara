"""Summarize each bounded scale-measurement process separately."""

from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path


def nearest_rank(values: list[int], percentile: int) -> int:
    return values[max(1, (len(values) * percentile + 99) // 100) - 1]


def summarize(path: Path) -> list[dict]:
    groups: dict[tuple, list[int]] = defaultdict(list)
    with path.open(encoding="utf-8") as source:
        for line in source:
            if not line.strip():
                continue
            row = json.loads(line)
            if "type" in row or "duration_ns" not in row:
                continue
            key = tuple(
                (name, row.get(name))
                for name in ("test", "population", "scenario", "interpolation", "batch_size")
            )
            groups[key].append(int(row["duration_ns"]))
    result = []
    for key, values in sorted(groups.items(), key=lambda item: str(item[0])):
        values.sort()
        result.append(
            {
                "source": path.name,
                "group": dict(key),
                "samples": len(values),
                "p50_ns": nearest_rank(values, 50),
                "p95_ns": nearest_rank(values, 95),
                "p99_ns": nearest_rank(values, 99) if len(values) >= 1000 else None,
                "min_ns": values[0],
                "max_ns": values[-1],
                "incomplete_for_p99": len(values) < 1000,
                "allocation_measurement": "not_collected",
            }
        )
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("files", nargs="+", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    summary = [row for path in args.files for row in summarize(path)]
    encoded = "\n".join(json.dumps(row, sort_keys=True) for row in summary) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
    else:
        print(encoded, end="")


if __name__ == "__main__":
    main()
