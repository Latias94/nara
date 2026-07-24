#!/usr/bin/env python3
"""Record one warm candidate desktop-present component without evaluating evidence."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys
from typing import Sequence

import candidate_measurement
import smoke_artifact


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--archive", type=Path, required=True)
    result.add_argument("--work-root", type=Path, required=True)
    result.add_argument("--output", type=Path, required=True)
    result.add_argument("--expected-platform", required=True)
    result.add_argument("--expected-source-revision", required=True)
    result.add_argument("--sample-index", type=int, required=True)
    result.add_argument("--desktop-launcher-json", default="[]")
    result.add_argument("--desktop-environment", action="append", default=[])
    return result


def main(arguments: Sequence[str] | None = None) -> int:
    options = parser().parse_args(arguments)
    try:
        launcher = tuple(smoke_artifact.parse_launcher(options.desktop_launcher_json))
        environment = smoke_artifact.parse_environment(options.desktop_environment)
        spec = candidate_measurement.CandidateProcessSpec(
            collector_id="desktop_probe_candidate_startup_v1",
            layout_key="desktop_probe",
            entry_point="tools/desktop-render-probe",
            marker_event="desktop_first_playable_present",
            component_boundary_id="desktop_first_playable_present_marker_received_v1",
            command_prefix=launcher,
            command_arguments=(),
            validate_stdout=candidate_measurement.validate_desktop_stdout,
        )
        succeeded = candidate_measurement.collect_candidate_startup(
            options.archive,
            options.work_root,
            options.output,
            options.expected_platform,
            options.expected_source_revision,
            options.sample_index,
            spec,
            environment,
        )
    except (candidate_measurement.CandidateMeasurementError, smoke_artifact.ArtifactError):
        print("candidate-measurement: input rejected", file=sys.stderr)
        return 1
    if not succeeded:
        print("candidate-measurement: failed sample recorded", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
