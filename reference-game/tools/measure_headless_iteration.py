#!/usr/bin/env python3
"""Record one warm candidate headless-startup component without evaluating evidence."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys
from typing import Sequence

import candidate_measurement


HEADLESS_SPEC = candidate_measurement.CandidateProcessSpec(
    collector_id="headless_candidate_startup_v1",
    layout_key="headless",
    entry_point="bin/headless",
    marker_event="headless_first_authoritative_tick",
    component_boundary_id="headless_first_authoritative_tick_marker_received_v1",
    command_prefix=(),
    command_arguments=("--max-ticks", "96"),
    validate_stdout=candidate_measurement.validate_headless_stdout,
)


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--archive", type=Path, required=True)
    result.add_argument("--work-root", type=Path, required=True)
    result.add_argument("--output", type=Path, required=True)
    result.add_argument("--expected-platform", required=True)
    result.add_argument("--expected-source-revision", required=True)
    result.add_argument("--sample-index", type=int, required=True)
    return result


def main(arguments: Sequence[str] | None = None) -> int:
    options = parser().parse_args(arguments)
    try:
        succeeded = candidate_measurement.collect_candidate_startup(
            options.archive,
            options.work_root,
            options.output,
            options.expected_platform,
            options.expected_source_revision,
            options.sample_index,
            HEADLESS_SPEC,
            {},
        )
    except candidate_measurement.CandidateMeasurementError:
        print("candidate-measurement: input rejected", file=sys.stderr)
        return 1
    if not succeeded:
        print("candidate-measurement: failed sample recorded", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
