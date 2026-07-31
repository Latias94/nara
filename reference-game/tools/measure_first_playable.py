#!/usr/bin/env python3
"""Collect and verify one isolated, non-decisive first-playable baseline."""

from __future__ import annotations

import argparse
from dataclasses import dataclass, replace
from datetime import UTC, datetime
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import signal
import shutil
import subprocess
import sys
import threading
import time
from typing import Any, Sequence


RUN_SCHEMA = "nara.reference-game.first-playable-collection-transport-v1"
RUN_FILENAME = "run-manifest.json"
RAW_FILENAME = "raw-samples.jsonl"
CATALOG_PATH = "docs/benchmarks/data/protocol/v1/reference-game-first-playable.json"
HELPER_PATH = "reference-game/tools/measure_first_playable.py"
COLLECTOR_ID = "u14"

MAX_GIT_OUTPUT = 256 * 1024
MAX_COMMAND_OUTPUT = 1024 * 1024
MAX_DIAGNOSTIC_LOG_BYTES = 64 * 1024 * 1024
MAX_CATALOG_BYTES = 256 * 1024
MAX_MANIFEST_BYTES = 256 * 1024
MAX_RAW_BYTES = 4 * 1024 * 1024
MAX_RAW_LINE_BYTES = 64 * 1024
MAX_RAW_SAMPLES = 4096
MAX_SOURCE_BYTES = 2 * 1024 * 1024
PROCESS_JOIN_SECONDS = 1.0
GIT_TIMEOUT_SECONDS = 10.0
DEFAULT_COMMAND_TIMEOUT_SECONDS = 30 * 60
CLEAN_STATUS_ARGUMENTS = ("status", "--porcelain=v1", "-z")

REQUIRED_PATHS = (
    "Cargo.toml",
    "reference-game/Cargo.toml",
    "reference-game/Cargo.lock",
    "reference-game/src/bin/headless.rs",
    "reference-game/src/bin/desktop.rs",
    "reference-game/src/bin/desktop_render_probe.rs",
    "reference-game/src/systems.rs",
    "reference-game/scenes/startup.scene.json",
    "reference-game/tests/public_surface.rs",
    HELPER_PATH,
    CATALOG_PATH,
)

AUTOMATIC_METRICS = frozenset(
    {
        "build.cold_ns",
        "build.incremental_ns",
        "gameplay.headless_wave_success",
        "iteration.body.p50_ns",
        "iteration.body.p95_ns",
        "iteration.data.p50_ns",
        "iteration.data.p95_ns",
        "iteration.structural.p50_ns",
        "iteration.structural.p95_ns",
        "journey.clean_to_headless_wave_ns",
    }
)
COVERAGE_NON_CLAIM = (
    "A passing static public-surface test is not an executed production-call denominator."
)
TWIN_METRICS = (
    ("iteration.body.p50_ns", "iteration.body.p95_ns"),
    ("iteration.data.p50_ns", "iteration.data.p95_ns"),
    ("iteration.structural.p50_ns", "iteration.structural.p95_ns"),
)
BUILD_ARGS = ("build", "--locked", "--bins", "--jobs", "1")
HEADLESS_ARGS = ("run", "--locked", "--bin", "headless", "--", "--max-ticks", "96")
PUBLIC_SURFACE_ARGS = ("test", "--locked", "--test", "public_surface", "--jobs", "1")

REQUIREMENT_FIELDS = (
    "id",
    "minimum_samples",
)
SCRATCH_DIRECTORIES = ("worktree", "target", "cargo-home", "home", "temp")
RUN_NON_CLAIMS = (
    "This local run does not grant release or publication authority.",
    "Missing manual or telemetry metrics remain missing; process exit is not substituted.",
    "Diagnostic logs are non-canonical and are not integrity evidence.",
    "Rust policy and oracle code owns environment compatibility, aggregation, and verdicts.",
)

BODY_BASE = b"""pub(crate) fn tick_project_weapons(mut weapons: Query<&mut Weapon, With<SceneEntitySource>>) {
    for mut weapon in &mut weapons {
        weapon.remaining_ticks = weapon.remaining_ticks.saturating_sub(1);
    }
}
"""
BODY_VARIANT = BODY_BASE.replace(b"saturating_sub(1)", b"saturating_sub(1_u64)")
STRUCTURAL_BASE = (
    b"""struct MovementIntentCommand {
    direction: MovementDirection,
    pressed: bool,
}
""",
    b"""Ok(MovementIntentCommand {
        direction,
        pressed: *pressed,
    })
""",
)
STRUCTURAL_VARIANT = (
    b"""struct MovementIntentCommand {
    direction: MovementDirection,
    pressed: bool,
    _measurement_generation: u8,
}
""",
    b"""Ok(MovementIntentCommand {
        direction,
        pressed: *pressed,
        _measurement_generation: 0,
    })
""",
)


class MeasurementError(RuntimeError):
    """A stable refusal from the local measurement helper."""


@dataclass(frozen=True)
class CommandResult:
    returncode: int
    output: bytes
    duration_ns: int
    completed_monotonic_ns: int
    started_at_utc: str
    completed_at_utc: str
    timed_out: bool = False
    overflowed: bool = False

    @property
    def failed(self) -> bool:
        return self.returncode != 0 or self.timed_out or self.overflowed


@dataclass(frozen=True)
class EditScenario:
    label: str
    path: Path
    base: bytes
    variant: bytes
    metric_ids: tuple[str, str]
    build: bool
    incremental_metric: str | None = None
    variant_summary_updates: tuple[tuple[str, int], ...] = ()


class OutputReader(threading.Thread):
    def __init__(self, stream: Any, limit: int) -> None:
        super().__init__(daemon=True)
        self.stream = stream
        self.limit = limit
        self.output = bytearray()
        self.overflowed = False
        self.error: Exception | None = None
        self.finished = threading.Event()

    def run(self) -> None:
        try:
            while chunk := self.stream.read1(8 * 1024):
                remaining = self.limit - len(self.output)
                if len(chunk) > remaining:
                    self.output.extend(chunk[: max(remaining, 0)])
                    self.overflowed = True
                    return
                self.output.extend(chunk)
        except (OSError, ValueError) as error:
            self.error = error
        finally:
            self.finished.set()


class WindowsJob:
    """Owns one Windows subprocess tree from its first instruction."""

    def __init__(self) -> None:
        import ctypes
        from ctypes import wintypes

        class BasicLimitInformation(ctypes.Structure):
            _fields_ = [
                ("per_process_user_time_limit", ctypes.c_longlong),
                ("per_job_user_time_limit", ctypes.c_longlong),
                ("limit_flags", wintypes.DWORD),
                ("minimum_working_set_size", ctypes.c_size_t),
                ("maximum_working_set_size", ctypes.c_size_t),
                ("active_process_limit", wintypes.DWORD),
                ("affinity", ctypes.c_size_t),
                ("priority_class", wintypes.DWORD),
                ("scheduling_class", wintypes.DWORD),
            ]

        class IoCounters(ctypes.Structure):
            _fields_ = [
                ("read_operation_count", ctypes.c_ulonglong),
                ("write_operation_count", ctypes.c_ulonglong),
                ("other_operation_count", ctypes.c_ulonglong),
                ("read_transfer_count", ctypes.c_ulonglong),
                ("write_transfer_count", ctypes.c_ulonglong),
                ("other_transfer_count", ctypes.c_ulonglong),
            ]

        class ExtendedLimitInformation(ctypes.Structure):
            _fields_ = [
                ("basic_limit_information", BasicLimitInformation),
                ("io_info", IoCounters),
                ("process_memory_limit", ctypes.c_size_t),
                ("job_memory_limit", ctypes.c_size_t),
                ("peak_process_memory_used", ctypes.c_size_t),
                ("peak_job_memory_used", ctypes.c_size_t),
            ]

        self.ctypes = ctypes
        self.wintypes = wintypes
        self.kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        self.kernel32.CloseHandle.argtypes = (wintypes.HANDLE,)
        self.kernel32.CloseHandle.restype = wintypes.BOOL
        self.kernel32.CreateJobObjectW.restype = wintypes.HANDLE
        self.kernel32.SetInformationJobObject.argtypes = (
            wintypes.HANDLE,
            ctypes.c_int,
            ctypes.c_void_p,
            wintypes.DWORD,
        )
        self.kernel32.SetInformationJobObject.restype = wintypes.BOOL
        self.handle = self.kernel32.CreateJobObjectW(None, None)
        if not self.handle:
            raise self._error("A Windows process job could not be created")
        limits = ExtendedLimitInformation()
        limits.basic_limit_information.limit_flags = 0x00002000
        if not self.kernel32.SetInformationJobObject(
            self.handle,
            9,
            ctypes.byref(limits),
            ctypes.sizeof(limits),
        ):
            error = self._error("A Windows process job could not be bounded")
            self.close()
            raise error

    def _error(self, message: str) -> MeasurementError:
        return MeasurementError(f"{message} (Win32 error {self.ctypes.get_last_error()}).")

    def attach_and_resume(self, process_id: int) -> None:
        ctypes = self.ctypes
        wintypes = self.wintypes
        kernel32 = self.kernel32
        kernel32.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
        kernel32.OpenProcess.restype = wintypes.HANDLE
        process_handle = kernel32.OpenProcess(0x0001 | 0x0100 | 0x0800, False, process_id)
        if not process_handle:
            raise self._error("A suspended Windows process could not be opened")
        try:
            kernel32.AssignProcessToJobObject.argtypes = (wintypes.HANDLE, wintypes.HANDLE)
            kernel32.AssignProcessToJobObject.restype = wintypes.BOOL
            if not kernel32.AssignProcessToJobObject(self.handle, process_handle):
                raise self._error("A suspended Windows process could not join its job")
            ntdll = ctypes.WinDLL("ntdll")
            ntdll.NtResumeProcess.argtypes = (wintypes.HANDLE,)
            ntdll.NtResumeProcess.restype = wintypes.LONG
            status = ntdll.NtResumeProcess(process_handle)
            if status != 0:
                raise MeasurementError(
                    f"A suspended Windows process could not resume (NTSTATUS {status:#x})."
                )
        finally:
            kernel32.CloseHandle(process_handle)

    def terminate(self) -> None:
        self.kernel32.TerminateJobObject.argtypes = (self.wintypes.HANDLE, self.wintypes.UINT)
        self.kernel32.TerminateJobObject.restype = self.wintypes.BOOL
        if self.handle is not None and not self.kernel32.TerminateJobObject(self.handle, 1):
            raise self._error("A Windows process job could not be terminated")

    def close(self) -> None:
        if self.handle is not None:
            self.kernel32.CloseHandle(self.handle)
            self.handle = None


def utc_now() -> str:
    return datetime.now(UTC).isoformat(timespec="microseconds").replace("+00:00", "Z")


def digest(contents: bytes) -> str:
    return hashlib.sha256(contents).hexdigest()


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")


def resolve(path: Path, purpose: str) -> Path:
    try:
        return path.resolve()
    except (OSError, RuntimeError) as error:
        raise MeasurementError(f"The {purpose} path could not be resolved: {error}") from error


def within(path: Path, parent: Path) -> bool:
    return path.is_relative_to(parent)


def read_bounded(path: Path, limit: int, purpose: str) -> bytes:
    try:
        with path.open("rb") as source:
            contents = source.read(limit + 1)
    except OSError as error:
        raise MeasurementError(f"The {purpose} could not be read: {error}") from error
    if len(contents) > limit:
        raise MeasurementError(f"The {purpose} exceeded its bounded byte limit.")
    return contents


def read_json(path: Path, limit: int, purpose: str) -> dict[str, Any]:
    try:
        value = json.loads(read_bounded(path, limit, purpose))
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise MeasurementError(f"The {purpose} is not valid bounded JSON.") from error
    if not isinstance(value, dict):
        raise MeasurementError(f"The {purpose} must be a JSON object.")
    return value


def write_bytes(path: Path, contents: bytes, purpose: str) -> None:
    try:
        path.write_bytes(contents)
    except OSError as error:
        raise MeasurementError(f"The {purpose} could not be written: {error}") from error


def write_json(path: Path, value: Any) -> None:
    temporary = path.with_name(f"{path.name}.tmp")
    encoded = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")
    try:
        temporary.write_bytes(encoded)
        temporary.replace(path)
    except OSError as error:
        raise MeasurementError(f"The `{path.name}` artifact could not be written: {error}") from error


class DiagnosticLogs:
    def __init__(self, byte_budget: int) -> None:
        self.remaining = byte_budget

    def write(self, path: Path, output: bytes) -> None:
        retained = output[: self.remaining]
        self.remaining -= len(retained)
        write_bytes(path, retained, "bounded command log")


def stop_process(process: subprocess.Popen[bytes], windows_job: WindowsJob | None) -> None:
    group_error: OSError | MeasurementError | None = None
    if windows_job is not None:
        try:
            windows_job.terminate()
        except MeasurementError as error:
            group_error = error
    elif os.name != "nt":
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        except OSError as error:
            group_error = error
    if process.poll() is None:
        try:
            process.kill()
        except OSError as error:
            group_error = group_error or error
    try:
        process.wait(timeout=PROCESS_JOIN_SECONDS)
    except subprocess.TimeoutExpired as error:
        raise MeasurementError("A bounded process did not stop within its deadline.") from error
    if group_error is not None:
        raise MeasurementError(f"A bounded process group could not be stopped: {group_error}")


def run_process(
    command: Sequence[str],
    cwd: Path,
    environment: dict[str, str],
    timeout_seconds: float,
    *,
    output_limit: int = MAX_COMMAND_OUTPUT,
    log_path: Path | None = None,
    diagnostic_logs: DiagnosticLogs | None = None,
) -> CommandResult:
    started_at = utc_now()
    started = time.monotonic_ns()
    windows_job: WindowsJob | None = None
    process: subprocess.Popen[bytes] | None = None
    try:
        process_options: dict[str, Any] = {}
        if os.name == "nt":
            windows_job = WindowsJob()
            process_options["creationflags"] = (
                getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
                | getattr(subprocess, "CREATE_SUSPENDED", 0x00000004)
            )
        else:
            process_options["start_new_session"] = True
        process = subprocess.Popen(
            list(command),
            cwd=cwd,
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            **process_options,
        )
        if windows_job is not None:
            windows_job.attach_and_resume(process.pid)
    except BaseException as error:
        if process is not None:
            try:
                stop_process(process, windows_job)
            except MeasurementError:
                pass
        if windows_job is not None:
            windows_job.close()
        if isinstance(error, (OSError, MeasurementError)):
            raise MeasurementError(f"A bounded process could not start: {error}") from error
        raise
    try:
        if process.stdout is None:
            stop_process(process, windows_job)
            raise MeasurementError("A bounded process did not expose readable output.")
        reader = OutputReader(process.stdout, output_limit)
        reader.start()
        deadline = time.monotonic() + timeout_seconds
        timed_out = False
        if not reader.finished.wait(timeout_seconds):
            timed_out = True
            stop_process(process, windows_job)
        elif reader.error is not None or reader.overflowed:
            stop_process(process, windows_job)
        else:
            remaining = max(deadline - time.monotonic(), 0.0)
            try:
                process.wait(timeout=remaining)
            except subprocess.TimeoutExpired:
                timed_out = True
                stop_process(process, windows_job)
        reader.join(PROCESS_JOIN_SECONDS)
        if reader.is_alive():
            try:
                process.stdout.close()
            except OSError:
                pass
            reader.join(PROCESS_JOIN_SECONDS)
        if reader.is_alive() or (
            reader.error is not None and not timed_out and not reader.overflowed
        ):
            raise MeasurementError("A bounded process output reader did not finish cleanly.")
        # Retire the Windows job or original POSIX process group after every command.
        stop_process(process, windows_job)
        completed_monotonic_ns = time.monotonic_ns()
        completed_at = utc_now()
        output = bytes(reader.output)
        if log_path is not None:
            if diagnostic_logs is None:
                write_bytes(log_path, output, "bounded command log")
            else:
                diagnostic_logs.write(log_path, output)
        return CommandResult(
            returncode=process.returncode if process.returncode is not None else -1,
            output=output,
            duration_ns=completed_monotonic_ns - started,
            completed_monotonic_ns=completed_monotonic_ns,
            started_at_utc=started_at,
            completed_at_utc=completed_at,
            timed_out=timed_out,
            overflowed=reader.overflowed,
        )
    except BaseException:
        try:
            stop_process(process, windows_job)
        except MeasurementError:
            pass
        raise
    finally:
        if windows_job is not None:
            windows_job.close()


def git_environment(optional_locks: str) -> dict[str, str]:
    environment = {
        key: value for key, value in os.environ.items() if not key.upper().startswith("GIT_")
    }
    environment["GIT_OPTIONAL_LOCKS"] = optional_locks
    return environment


def run_git(
    subject: Path,
    arguments: Sequence[str],
    *,
    output_limit: int = MAX_GIT_OUTPUT,
) -> bytes:
    result = run_process(
        ["git", "-C", os.fspath(subject), *arguments],
        subject,
        git_environment("0"),
        GIT_TIMEOUT_SECONDS,
        output_limit=output_limit,
    )
    if result.failed:
        detail = result.output.decode("utf-8", errors="replace").strip()
        raise MeasurementError(f"Git subject inspection failed: {detail or 'unknown Git error'}")
    return result.output


def clean_subject(argument: Path) -> tuple[Path, str, str]:
    try:
        if not argument.is_dir():
            raise MeasurementError("The measurement subject must be an existing directory.")
    except OSError as error:
        raise MeasurementError(f"The measurement subject could not be inspected: {error}") from error
    subject = resolve(argument, "measurement subject")
    reported = run_git(subject, ["rev-parse", "--show-toplevel"])
    if resolve(Path(reported.decode("utf-8").strip()), "Git repository root") != subject:
        raise MeasurementError("The measurement subject must be the repository root.")
    if run_git(subject, CLEAN_STATUS_ARGUMENTS):
        raise MeasurementError("The measurement subject must be clean.")
    revision = run_git(subject, ["rev-parse", "--verify", "HEAD"]).decode("utf-8").strip()
    if len(revision) != 40 or any(character not in "0123456789abcdef" for character in revision):
        raise MeasurementError("The measurement subject did not report one full revision.")
    missing = [
        relative
        for relative in REQUIRED_PATHS
        if not (subject / relative).is_file() or (subject / relative).is_symlink()
    ]
    if missing:
        raise MeasurementError(f"The measurement subject lacks required public paths: {', '.join(missing)}")
    helper = resolve(subject / HELPER_PATH, "measurement helper")
    if resolve(Path(__file__), "executing measurement helper") != helper:
        raise MeasurementError("The executing helper must come from the measurement subject.")
    helper_bytes = read_bounded(helper, MAX_SOURCE_BYTES, "measurement helper")
    committed = run_git(
        subject,
        ["show", f"{revision}:{HELPER_PATH}"],
        output_limit=MAX_SOURCE_BYTES,
    )
    if committed != helper_bytes:
        raise MeasurementError("The executing helper bytes must match the subject HEAD blob.")
    return subject, revision, digest(helper_bytes)


def new_output(subject: Path, argument: Path) -> Path:
    output = resolve(argument, "measurement output")
    if within(output, subject):
        raise MeasurementError("The measurement output must live outside the measurement subject.")
    if output.exists():
        raise MeasurementError("The measurement output directory must not already exist.")
    if not output.parent.is_dir():
        raise MeasurementError("The measurement output parent must be an existing directory.")
    return output


def load_requirements(subject: Path) -> tuple[dict[str, dict[str, Any]], str]:
    catalog_path = subject / CATALOG_PATH
    catalog_bytes = read_bounded(catalog_path, MAX_CATALOG_BYTES, "metric catalog")
    try:
        catalog = json.loads(catalog_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise MeasurementError("The metric catalog is not valid JSON.") from error
    if not isinstance(catalog, dict):
        raise MeasurementError("The metric catalog root is not an object.")
    metrics = catalog.get("metrics")
    if not isinstance(metrics, list):
        raise MeasurementError("The metric catalog lacks its metrics array.")
    requirements: dict[str, dict[str, Any]] = {}
    for metric in metrics:
        if not isinstance(metric, dict) or metric.get("collector") != COLLECTOR_ID:
            continue
        metric_id = metric.get("id")
        minimum = metric.get("minimum_samples")
        if (
            not isinstance(metric_id, str)
            or not isinstance(minimum, int)
            or isinstance(minimum, bool)
            or minimum <= 0
            or minimum > MAX_RAW_SAMPLES
            or metric_id in requirements
        ):
            raise MeasurementError("The metric catalog contains an invalid U14 requirement.")
        requirements[metric_id] = {field: metric[field] for field in REQUIREMENT_FIELDS}
    if not AUTOMATIC_METRICS.issubset(requirements):
        raise MeasurementError("The metric catalog lacks an automatic U14 metric.")
    for left, right in TWIN_METRICS:
        if requirements[left]["minimum_samples"] != requirements[right]["minimum_samples"]:
            raise MeasurementError("A P50/P95 metric pair has different sample floors.")
    if sum(requirements[metric]["minimum_samples"] for metric in AUTOMATIC_METRICS) > MAX_RAW_SAMPLES:
        raise MeasurementError("The automatic measurement population exceeds its sample budget.")
    return requirements, digest(catalog_bytes)


def isolated_environment(output: Path, target: Path) -> dict[str, str]:
    allowed = {
        "ALL_PROXY",
        "COMSPEC",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "INCLUDE",
        "LIB",
        "LIBPATH",
        "NO_PROXY",
        "PATH",
        "PATHEXT",
        "RUSTUP_HOME",
        "RUSTUP_TOOLCHAIN",
        "SSL_CERT_DIR",
        "SSL_CERT_FILE",
        "SYSTEMROOT",
        "WINDIR",
    }
    environment = {
        key.upper(): value for key, value in os.environ.items() if key.upper() in allowed
    }
    if "RUSTUP_HOME" not in environment:
        for home_key in ("USERPROFILE", "HOME"):
            home = os.environ.get(home_key)
            candidate = Path(home) / ".rustup" if home else None
            if candidate is not None and candidate.is_dir():
                environment["RUSTUP_HOME"] = os.fspath(candidate)
                break
    home = output / "home"
    temporary = output / "temp"
    cargo_home = output / "cargo-home"
    app_data = home / "app-data"
    local_data = home / "local-data"
    for directory in (home, temporary, cargo_home, app_data, local_data):
        try:
            directory.mkdir(parents=True, exist_ok=True)
        except OSError as error:
            raise MeasurementError(f"The isolated environment could not be created: {error}") from error
    environment.update(
        {
            "APPDATA": os.fspath(app_data),
            "CARGO_BUILD_JOBS": "1",
            "CARGO_HOME": os.fspath(cargo_home),
            "CARGO_INCREMENTAL": "1",
            "CARGO_TARGET_DIR": os.fspath(target),
            "CARGO_TERM_COLOR": "never",
            "HOME": os.fspath(home),
            "LOCALAPPDATA": os.fspath(local_data),
            "TEMP": os.fspath(temporary),
            "TMP": os.fspath(temporary),
            "TMPDIR": os.fspath(temporary),
            "USERPROFILE": os.fspath(home),
        }
    )
    return environment


def native_anchor(contents: bytes, anchor: bytes) -> bytes:
    return anchor.replace(b"\n", b"\r\n") if b"\r\n" in contents else anchor


def rust_variant(contents: bytes, base: Sequence[bytes], variant: Sequence[bytes]) -> bytes:
    changed = contents
    for old, new in zip(base, variant, strict=True):
        old = native_anchor(changed, old)
        new = native_anchor(changed, new)
        if changed.count(old) != 1:
            raise MeasurementError("A reviewed Rust edit anchor is no longer unique.")
        changed = changed.replace(old, new, 1)
    return changed


def scene_variant(contents: bytes) -> bytes:
    try:
        document = json.loads(contents)
        entities = document["payload"]["entities"]
    except (UnicodeDecodeError, json.JSONDecodeError, KeyError, TypeError, RecursionError) as error:
        raise MeasurementError("The startup scene lacks its expected entity list.") from error
    if not isinstance(entities, list):
        raise MeasurementError("The startup scene entity list is not an array.")
    players = [
        entity["components"]["reference_game.Player"]
        for entity in entities
        if isinstance(entity, dict)
        and isinstance(entity.get("components"), dict)
        and "reference_game.Player" in entity["components"]
    ]
    if len(players) != 1:
        raise MeasurementError("The startup scene must contain exactly one Player component.")
    try:
        hit_points = players[0]["value"]["value"]["hit-points"]
    except (KeyError, TypeError) as error:
        raise MeasurementError("The Player component lacks its hit-points field.") from error
    if hit_points != {"type": "i64", "value": 20}:
        raise MeasurementError("The Player hit-points field is not the expected i64 value 20.")
    hit_points["value"] = 21
    text = json.dumps(document, indent=2, ensure_ascii=True) + "\n"
    if b"\r\n" in contents:
        text = text.replace("\n", "\r\n")
    return text.encode("utf-8")


def validate_terminal_summary(value: object) -> dict[str, Any]:
    fields = {
        "schema",
        "outcome",
        "tick",
        "score",
        "player_hit_points",
        "enemies_remaining",
        "projectiles_remaining",
    }
    if not isinstance(value, dict) or set(value) != fields:
        raise MeasurementError("The public headless summary has an invalid field set.")
    if value.get("schema") != "nara-reference-game.wave-summary-v1":
        raise MeasurementError("The public headless summary has an unknown schema.")
    if value.get("outcome") != "completed":
        raise MeasurementError("The public headless summary is not the completed reference wave.")
    numeric_fields = (
        "tick",
        "score",
        "player_hit_points",
        "enemies_remaining",
        "projectiles_remaining",
    )
    if any(
        not isinstance(value[field], int) or isinstance(value[field], bool)
        for field in numeric_fields
    ):
        raise MeasurementError("The public headless summary has a non-integer state field.")
    if (
        value["tick"] <= 0
        or value["score"] < 0
        or value["player_hit_points"] <= 0
        or value["enemies_remaining"] != 0
        or value["projectiles_remaining"] < 0
    ):
        raise MeasurementError("The public headless summary is not a valid completed state.")
    return value


def terminal_summary(output: bytes) -> tuple[dict[str, Any], str]:
    for line in reversed(output.splitlines()):
        try:
            value = json.loads(line)
        except (UnicodeDecodeError, json.JSONDecodeError, RecursionError):
            continue
        if isinstance(value, dict) and value.get("schema") == "nara-reference-game.wave-summary-v1":
            summary = validate_terminal_summary(value)
            return summary, digest(canonical_json(summary))
    raise MeasurementError("The public headless command emitted no terminal wave summary.")


def reset_directory(path: Path, owner: Path) -> None:
    path = resolve(path, "measurement target")
    owner = resolve(owner, "measurement output")
    if path == owner or not within(path, owner):
        raise MeasurementError("The measurement target escaped its output root.")
    try:
        if path.exists():
            shutil.rmtree(path)
        path.mkdir(parents=True)
    except OSError as error:
        raise MeasurementError(f"The measurement target could not be reset: {error}") from error


def remove_owned_directory(path: Path, owner: Path) -> None:
    owner = resolve(owner, "measurement output")
    if path.is_symlink():
        raise MeasurementError("A measurement scratch path became a symbolic link.")
    if not path.exists():
        return
    path = resolve(path, "measurement scratch")
    if path == owner or not within(path, owner) or not path.is_dir():
        raise MeasurementError("A measurement scratch path escaped its output root.")
    try:
        shutil.rmtree(path)
    except OSError as error:
        raise MeasurementError(f"A measurement scratch directory could not be removed: {error}") from error


def cleanup_worktree(
    subject: Path,
    worktree: Path,
    output: Path,
    diagnostic_logs: DiagnosticLogs,
) -> None:
    registered = run_git(subject, ["worktree", "list", "--porcelain"])
    paths = []
    for line in registered.decode("utf-8", errors="strict").splitlines():
        if line.startswith("worktree "):
            paths.append(resolve(Path(line.removeprefix("worktree ")), "registered worktree"))
    resolved_worktree = resolve(worktree, "measurement worktree")
    if resolved_worktree in paths:
        removal = run_process(
            [
                "git",
                "-C",
                os.fspath(subject),
                "worktree",
                "remove",
                "--force",
                os.fspath(worktree),
            ],
            subject,
            git_environment("1"),
            60.0,
            output_limit=MAX_GIT_OUTPUT,
            log_path=output / "logs/cleanup-worktree.log",
            diagnostic_logs=diagnostic_logs,
        )
        if removal.failed:
            raise MeasurementError("The detached worktree could not be removed.")
    remove_owned_directory(worktree, output)


class Collector:
    def __init__(
        self,
        game: Path,
        output: Path,
        cargo: str,
        environment: dict[str, str],
        timeout: float,
        revision: str,
        requirements: dict[str, dict[str, Any]],
        diagnostic_logs: DiagnosticLogs,
    ) -> None:
        self.game = game
        self.output = output
        self.cargo = cargo
        self.environment = environment
        self.timeout = timeout
        self.revision = revision
        self.requirements = requirements
        self.diagnostic_logs = diagnostic_logs
        self.records: list[dict[str, Any]] = []
        self.indices: dict[str, int] = {}

    def count(self, *metric_ids: str) -> int:
        return max(self.requirements[metric_id]["minimum_samples"] for metric_id in metric_ids)

    def command(
        self,
        arguments: Sequence[str],
        log_name: str,
    ) -> tuple[CommandResult, str]:
        reference = f"logs/{log_name}"
        result = run_process(
            [self.cargo, *arguments],
            self.game,
            self.environment,
            self.timeout,
            log_path=self.output / reference,
            diagnostic_logs=self.diagnostic_logs,
        )
        return result, reference

    def record(
        self,
        metric_ids: Sequence[str],
        result: CommandResult,
        value: int | None,
        log_reference: str,
        *,
        result_digest: str | None = None,
        semantic_failure: bool = False,
    ) -> None:
        failed = result.failed or semantic_failure or value is None
        for metric_id in metric_ids:
            index = self.indices.get(metric_id, 0) + 1
            self.indices[metric_id] = index
            self.records.append(
                {
                    "metric_id": metric_id,
                    "sample_index": index,
                    "sample_value": None if failed else value,
                    "started_at_utc": result.started_at_utc,
                    "completed_at_utc": result.completed_at_utc,
                    "source_revision": self.revision,
                    "command": {
                        "exit_code": result.returncode,
                        "timed_out": result.timed_out,
                        "output_overflowed": result.overflowed,
                        "diagnostic_log": log_reference,
                    },
                    "result_digest": result_digest,
                }
            )


def measured_from(result: CommandResult, started_at: str, started_ns: int) -> CommandResult:
    return replace(
        result,
        started_at_utc=started_at,
        duration_ns=result.completed_monotonic_ns - started_ns,
    )


def collect_edit_population(
    collector: Collector,
    scenario: EditScenario,
    baseline: dict[str, Any],
) -> None:
    count = collector.count(*scenario.metric_ids)
    if scenario.incremental_metric is not None:
        count = max(count, collector.count(scenario.incremental_metric))
    try:
        for sample in range(1, count + 1):
            use_variant = sample % 2 == 1
            write_bytes(
                scenario.path,
                scenario.variant if use_variant else scenario.base,
                f"{scenario.label} edit",
            )
            started_at = utc_now()
            started_ns = time.monotonic_ns()
            if scenario.build:
                build_result, build_log = collector.command(
                    BUILD_ARGS,
                    f"{scenario.label}-build-{sample:02}.log",
                )
                if (
                    scenario.incremental_metric is not None
                    and sample <= collector.count(scenario.incremental_metric)
                ):
                    collector.record(
                        (scenario.incremental_metric,),
                        build_result,
                        build_result.duration_ns,
                        build_log,
                    )
                if build_result.failed:
                    collector.record(
                        scenario.metric_ids,
                        measured_from(build_result, started_at, started_ns),
                        None,
                        build_log,
                    )
                    raise MeasurementError(f"The {scenario.label}-edit build failed.")
            headless, headless_log = collector.command(
                HEADLESS_ARGS,
                f"{scenario.label}-headless-{sample:02}.log",
            )
            measured = measured_from(headless, started_at, started_ns)
            if headless.failed:
                collector.record(
                    scenario.metric_ids,
                    measured,
                    None,
                    headless_log,
                )
                raise MeasurementError(f"The {scenario.label}-edit headless run failed.")
            try:
                summary, summary_digest = terminal_summary(headless.output)
                expected = dict(baseline)
                if use_variant:
                    expected.update(scenario.variant_summary_updates)
                if summary != expected:
                    raise MeasurementError(
                        f"The {scenario.label} edit changed an unexpected game result."
                    )
            except MeasurementError:
                collector.record(
                    scenario.metric_ids,
                    measured,
                    None,
                    headless_log,
                    semantic_failure=True,
                )
                raise
            collector.record(
                scenario.metric_ids,
                measured,
                measured.duration_ns,
                headless_log,
                result_digest=summary_digest,
            )
    finally:
        write_bytes(
            scenario.path,
            scenario.base,
            f"{scenario.label} edit restoration",
        )


def command_error(result: CommandResult) -> str | None:
    if result.timed_out:
        return "timed out"
    if result.overflowed:
        return "exceeded its output budget"
    if result.returncode != 0:
        return f"exited with status {result.returncode}"
    return None


def collect_first_playable(
    subject_argument: Path,
    output_argument: Path,
    cargo: str,
    timeout: float,
) -> None:
    subject, revision, collector_sha256 = clean_subject(subject_argument)
    output = new_output(subject, output_argument)
    requirements, catalog_sha256 = load_requirements(subject)
    try:
        output.mkdir()
        (output / "logs").mkdir()
    except OSError as error:
        raise MeasurementError(f"The measurement output could not be created: {error}") from error
    worktree = output / "worktree"
    target = output / "target"
    diagnostic_logs = DiagnosticLogs(MAX_DIAGNOSTIC_LOG_BYTES)
    environment: dict[str, str] = {}
    originals: dict[Path, bytes] = {}
    records: list[dict[str, Any]] = []
    environment_record: dict[str, Any] = {}
    checks: dict[str, Any] = {}
    baseline: dict[str, Any] | None = None
    baseline_digest: str | None = None
    collector: Collector | None = None
    collection_error: MeasurementError | None = None
    started_at = utc_now()

    try:
        environment = isolated_environment(output, target)
        add = run_process(
            ["git", "-C", os.fspath(subject), "worktree", "add", "--detach", os.fspath(worktree), revision],
            subject,
            git_environment("1"),
            60.0,
            output_limit=MAX_GIT_OUTPUT,
            log_path=output / "logs/setup-worktree.log",
            diagnostic_logs=diagnostic_logs,
        )
        if add.failed:
            raise MeasurementError("The detached measurement worktree could not be created.")
        game = worktree / "reference-game"
        systems = game / "src/systems.rs"
        scene = game / "scenes/startup.scene.json"
        originals = {
            systems: read_bounded(systems, MAX_SOURCE_BYTES, "systems source"),
            scene: read_bounded(scene, MAX_SOURCE_BYTES, "startup scene"),
        }
        body = rust_variant(originals[systems], (BODY_BASE,), (BODY_VARIANT,))
        structural = rust_variant(originals[systems], STRUCTURAL_BASE, STRUCTURAL_VARIANT)
        data = scene_variant(originals[scene])

        fetch = run_process(
            [cargo, "fetch", "--locked"],
            game,
            environment,
            timeout,
            log_path=output / "logs/setup-cargo-fetch.log",
            diagnostic_logs=diagnostic_logs,
        )
        if fetch.failed:
            raise MeasurementError("The isolated dependency fetch failed.")
        environment["CARGO_NET_OFFLINE"] = "true"
        cargo_version = run_process(
            [cargo, "--version"],
            game,
            environment,
            30.0,
            log_path=output / "logs/environment-cargo-version.log",
            diagnostic_logs=diagnostic_logs,
        )
        rustc_version = run_process(
            ["rustc", "--version", "--verbose"],
            game,
            environment,
            30.0,
            log_path=output / "logs/environment-rustc-version.log",
            diagnostic_logs=diagnostic_logs,
        )
        if cargo_version.failed or rustc_version.failed:
            raise MeasurementError("The isolated toolchain identity could not be inspected.")
        environment_record = {
            "os_name": platform.system(),
            "os_release": platform.release(),
            "runner_image": os.environ.get("ImageOS") or os.environ.get("RUNNER_OS") or "local",
            "rustc_version": rustc_version.output.decode(errors="replace").strip(),
            "cargo_version": cargo_version.output.decode(errors="replace").strip(),
            "cpu_model": (
                platform.processor()
                or os.environ.get("PROCESSOR_IDENTIFIER")
                or platform.machine()
            ),
            "build_profile": "debug",
            "desktop_adapter_or_software_profile": "not_collected",
            "cargo_build_jobs": 1,
            "cargo_network": "offline_after_fetch",
        }
        collector = Collector(
            game,
            output,
            cargo,
            environment,
            timeout,
            revision,
            requirements,
            diagnostic_logs,
        )

        cold_count = collector.count("build.cold_ns")
        journey_count = collector.count("journey.clean_to_headless_wave_ns")
        success_count = collector.count("gameplay.headless_wave_success")
        for sample in range(1, max(cold_count, journey_count, success_count) + 1):
            reset_directory(target, output)
            journey_started_at = utc_now()
            journey_started_ns = time.monotonic_ns()
            build_result, build_log = collector.command(
                BUILD_ARGS,
                f"cold-build-{sample:02}.log",
            )
            if sample <= cold_count:
                collector.record(
                    ("build.cold_ns",),
                    build_result,
                    build_result.duration_ns,
                    build_log,
                )
            if build_result.failed:
                raise MeasurementError(f"A cold reference-game build {command_error(build_result)}.")
            if sample > max(journey_count, success_count):
                continue
            headless, headless_log = collector.command(
                HEADLESS_ARGS,
                f"clean-headless-{sample:02}.log",
            )
            measured = measured_from(headless, journey_started_at, journey_started_ns)
            if headless.failed:
                if sample <= journey_count:
                    collector.record(
                        ("journey.clean_to_headless_wave_ns",),
                        measured,
                        None,
                        headless_log,
                    )
                if sample <= success_count:
                    collector.record(
                        ("gameplay.headless_wave_success",),
                        headless,
                        None,
                        headless_log,
                    )
                raise MeasurementError(f"A clean headless run {command_error(headless)}.")
            try:
                summary, summary_digest = terminal_summary(headless.output)
                if baseline is None:
                    baseline, baseline_digest = summary, summary_digest
                elif summary != baseline:
                    raise MeasurementError("Clean headless runs produced different terminal states.")
            except MeasurementError:
                if sample <= journey_count:
                    collector.record(
                        ("journey.clean_to_headless_wave_ns",),
                        measured,
                        None,
                        headless_log,
                        semantic_failure=True,
                    )
                if sample <= success_count:
                    collector.record(
                        ("gameplay.headless_wave_success",),
                        headless,
                        None,
                        headless_log,
                        semantic_failure=True,
                    )
                raise
            if sample <= journey_count:
                collector.record(
                    ("journey.clean_to_headless_wave_ns",),
                    measured,
                    measured.duration_ns,
                    headless_log,
                    result_digest=summary_digest,
                )
            if sample <= success_count:
                collector.record(
                    ("gameplay.headless_wave_success",),
                    headless,
                    1,
                    headless_log,
                    result_digest=summary_digest,
                )
        if baseline is None:
            raise MeasurementError("No clean headless baseline was collected.")

        collect_edit_population(
            collector,
            EditScenario(
                label="body",
                path=systems,
                base=originals[systems],
                variant=body,
                metric_ids=("iteration.body.p50_ns", "iteration.body.p95_ns"),
                build=True,
                incremental_metric="build.incremental_ns",
            ),
            baseline,
        )
        collect_edit_population(
            collector,
            EditScenario(
                label="data",
                path=scene,
                base=originals[scene],
                variant=data,
                metric_ids=("iteration.data.p50_ns", "iteration.data.p95_ns"),
                build=False,
                variant_summary_updates=(("player_hit_points", 21),),
            ),
            baseline,
        )
        collect_edit_population(
            collector,
            EditScenario(
                label="structural",
                path=systems,
                base=originals[systems],
                variant=structural,
                metric_ids=(
                    "iteration.structural.p50_ns",
                    "iteration.structural.p95_ns",
                ),
                build=True,
            ),
            baseline,
        )
        coverage, coverage_log = collector.command(
            PUBLIC_SURFACE_ARGS,
            "public-surface-check.log",
        )
        checks["public_surface"] = {
            "exit_code": coverage.returncode,
            "timed_out": coverage.timed_out,
            "output_overflowed": coverage.overflowed,
            "diagnostic_log": coverage_log,
            "metric_admitted": False,
            "reason": COVERAGE_NON_CLAIM,
        }
        if coverage.failed:
            raise MeasurementError("The public-surface check failed.")
        records = collector.records
    except MeasurementError as error:
        collection_error = error
        if collector is not None:
            records = collector.records
    finally:
        cleanup_error: MeasurementError | None = None
        for path, contents in originals.items():
            try:
                path.write_bytes(contents)
            except OSError as error:
                cleanup_error = MeasurementError(f"An isolated edit could not be restored: {error}")
        try:
            cleanup_worktree(subject, worktree, output, diagnostic_logs)
        except (UnicodeDecodeError, MeasurementError) as error:
            cleanup_error = MeasurementError(f"The detached worktree cleanup failed: {error}")
        for name in SCRATCH_DIRECTORIES:
            try:
                remove_owned_directory(output / name, output)
            except MeasurementError as error:
                cleanup_error = error
        try:
            clean_after, revision_after, collector_after = clean_subject(subject)
            if (
                clean_after != subject
                or revision_after != revision
                or collector_after != collector_sha256
            ):
                cleanup_error = MeasurementError("The source repository changed during collection.")
        except MeasurementError as error:
            cleanup_error = error
        if cleanup_error is not None:
            collection_error = cleanup_error

        raw = b"".join(canonical_json(record) + b"\n" for record in records)
        if len(records) > MAX_RAW_SAMPLES or len(raw) > MAX_RAW_BYTES:
            collection_error = MeasurementError("The raw sample budget was exceeded.")
        write_bytes(output / RAW_FILENAME, raw, "raw sample artifact")
        write_json(
            output / RUN_FILENAME,
            {
                "schema": RUN_SCHEMA,
                "status": "failed" if collection_error else "collected",
                "source_revision": revision,
                "collector_sha256": collector_sha256,
                "started_at_utc": started_at,
                "completed_at_utc": utc_now(),
                "environment": environment_record,
                "metric_catalog_sha256": catalog_sha256,
                "raw_samples": {
                    "path": RAW_FILENAME,
                    "count": len(records),
                    "sha256": digest(raw),
                },
                "diagnostic_logs": {"directory": "logs", "canonical": False},
                "base_terminal_summary": baseline,
                "base_terminal_summary_digest": baseline_digest,
                "checks": checks,
                "failure": str(collection_error) if collection_error else None,
                "non_claims": list(RUN_NON_CLAIMS),
            },
        )
    verify_transport(
        subject,
        revision,
        collector_sha256,
        catalog_sha256,
        output,
    )
    if collection_error is not None:
        raise collection_error


def is_sha256(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def read_raw_records(run: Path, descriptor: object, revision: str) -> list[dict[str, Any]]:
    if (
        not isinstance(descriptor, dict)
        or set(descriptor) != {"path", "count", "sha256"}
        or descriptor.get("path") != RAW_FILENAME
        or not isinstance(descriptor.get("count"), int)
        or isinstance(descriptor.get("count"), bool)
        or not 0 <= descriptor["count"] <= MAX_RAW_SAMPLES
        or not is_sha256(descriptor.get("sha256"))
    ):
        raise MeasurementError("The raw-sample descriptor is invalid.")
    raw = read_bounded(run / RAW_FILENAME, MAX_RAW_BYTES, "raw sample artifact")
    if digest(raw) != descriptor["sha256"]:
        raise MeasurementError("The raw-sample digest does not match.")
    lines = raw.splitlines()
    if len(lines) != descriptor["count"] or any(not line for line in lines):
        raise MeasurementError("The raw-sample count does not match.")
    records = []
    for line in lines:
        if len(line) > MAX_RAW_LINE_BYTES:
            raise MeasurementError("A raw-sample line exceeded its byte budget.")
        try:
            record = json.loads(line)
        except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
            raise MeasurementError("A raw-sample line is not valid JSON.") from error
        if not isinstance(record, dict) or record.get("source_revision") != revision:
            raise MeasurementError("A raw sample has an invalid source identity.")
        metric_id = record.get("metric_id")
        sample_index = record.get("sample_index")
        sample_value = record.get("sample_value")
        command = record.get("command")
        if (
            not isinstance(metric_id, str)
            or not metric_id
            or not isinstance(sample_index, int)
            or isinstance(sample_index, bool)
            or sample_index <= 0
            or (
                sample_value is not None
                and (
                    not isinstance(sample_value, int)
                    or isinstance(sample_value, bool)
                    or sample_value < 0
                )
            )
            or not isinstance(command, dict)
            or set(command)
            != {"exit_code", "timed_out", "output_overflowed", "diagnostic_log"}
            or not isinstance(command.get("exit_code"), int)
            or isinstance(command.get("exit_code"), bool)
            or not isinstance(command.get("timed_out"), bool)
            or not isinstance(command.get("output_overflowed"), bool)
            or not isinstance(command.get("diagnostic_log"), str)
            or not command["diagnostic_log"].startswith("logs/")
        ):
            raise MeasurementError("A raw sample has an invalid transport shape.")
        records.append(record)
    return records


def verify_transport(
    subject: Path,
    revision: str,
    collector_sha256: str,
    catalog_sha256: str,
    argument: Path,
) -> None:
    if argument.is_symlink():
        raise MeasurementError("The measurement run must not be a symbolic link.")
    run = resolve(argument, "measurement run")
    if within(run, subject) or not run.is_dir():
        raise MeasurementError("The measurement run must be an external regular directory.")
    manifest = read_json(run / RUN_FILENAME, MAX_MANIFEST_BYTES, "run manifest")
    expected_fields = {
        "schema",
        "status",
        "source_revision",
        "collector_sha256",
        "started_at_utc",
        "completed_at_utc",
        "environment",
        "metric_catalog_sha256",
        "raw_samples",
        "diagnostic_logs",
        "base_terminal_summary",
        "base_terminal_summary_digest",
        "checks",
        "failure",
        "non_claims",
    }
    status = manifest.get("status")
    failure = manifest.get("failure")
    diagnostic_logs = manifest.get("diagnostic_logs")
    if (
        set(manifest) != expected_fields
        or manifest.get("schema") != RUN_SCHEMA
        or status not in {"collected", "failed"}
        or manifest.get("source_revision") != revision
        or manifest.get("collector_sha256") != collector_sha256
        or not isinstance(manifest.get("environment"), dict)
        or not isinstance(manifest.get("checks"), dict)
        or not isinstance(manifest.get("non_claims"), list)
        or not all(isinstance(item, str) for item in manifest["non_claims"])
        or diagnostic_logs != {"directory": "logs", "canonical": False}
        or (status == "collected" and failure is not None)
        or (status == "failed" and (not isinstance(failure, str) or not failure))
    ):
        raise MeasurementError("The run manifest has an invalid transport contract.")
    if manifest.get("metric_catalog_sha256") != catalog_sha256:
        raise MeasurementError("The run uses a different metric catalog.")
    read_raw_records(run, manifest.get("raw_samples"), revision)


def verify_run(subject_argument: Path, argument: Path) -> None:
    subject, revision, collector_sha256 = clean_subject(subject_argument)
    _, catalog_sha256 = load_requirements(subject)
    verify_transport(
        subject,
        revision,
        collector_sha256,
        catalog_sha256,
        argument,
    )


def positive_seconds(value: str) -> float:
    try:
        result = float(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("must be a number") from error
    if not math.isfinite(result) or result <= 0:
        raise argparse.ArgumentTypeError("must be finite and greater than zero")
    return result


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(
        description=(
            "Collect or verify one isolated reference-game baseline; this helper "
            "does not evaluate a performance or product result."
        )
    )
    commands = result.add_subparsers(dest="command", required=True)
    collect = commands.add_parser("collect", help="collect the automatic slice")
    collect.add_argument("--subject", type=Path, required=True)
    collect.add_argument("--output", type=Path, required=True)
    collect.add_argument("--cargo", default="cargo")
    collect.add_argument(
        "--command-timeout-seconds",
        type=positive_seconds,
        default=DEFAULT_COMMAND_TIMEOUT_SECONDS,
    )
    verify = commands.add_parser("verify", help="verify one bounded local transport")
    verify.add_argument("--subject", type=Path, required=True)
    verify.add_argument("--run", type=Path, required=True)
    return result


def main(arguments: Sequence[str] | None = None) -> int:
    options = parser().parse_args(arguments)
    try:
        if options.command == "collect":
            collect_first_playable(
                options.subject,
                options.output,
                options.cargo,
                options.command_timeout_seconds,
            )
        elif options.command == "verify":
            verify_run(options.subject, options.run)
        else:
            raise AssertionError(f"unhandled command: {options.command}")
    except MeasurementError as error:
        print(f"measurement-helper: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
