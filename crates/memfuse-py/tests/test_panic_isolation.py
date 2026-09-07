"""
FFI Panic Isolation Tests & Regression Guards for memfuse-py in Release Builds.

BEFORE / AFTER CONTRAST:
------------------------
BEFORE (Workspace Cargo.toml with global `panic = "abort"` in [profile.release]):
  When memfuse-py was part of the main workspace, Cargo forced `panic = "abort"`
  on all release artifacts (`maturin build --release`).
  In release builds, `std::panic::catch_unwind` inside `run_blocking_ffi` was WIRKUNGSLOS:
  any Rust panic immediately aborted the CPython process via SIGABRT (exit code 134 / signal 6)
  before catch_unwind could intercept it.

AFTER (Standalone Workspace in `crates/memfuse-py` with `panic = "unwind"` in [profile.release]):
  `crates/memfuse-py` is decoupled as an independent workspace with its own [profile.release]
  defining `panic = "unwind"`. `std::panic::catch_unwind` in `run_blocking_ffi` cleanly catches
  Rust panics across FFI boundaries in both debug and release builds, raising a catchable
  `PyRuntimeError` in Python without process termination.
"""

import sys
import subprocess
import pytest
from memfuse import _memfuse

def test_rust_panic_converted_to_pyruntimeerror():
    """Verifies that a Rust panic triggered inside FFI is converted to PyRuntimeError
    and caught as a standard Python exception without crashing the host process.

    This test confirms that `panic = "unwind"` is active in release mode so that
    `std::panic::catch_unwind` intercepts the Rust panic at the FFI boundary.
    """
    with pytest.raises(RuntimeError) as excinfo:
        _memfuse._trigger_panic_for_test("Custom test panic message")

    err_msg = str(excinfo.value)
    assert "Rust panic caught at FFI boundary" in err_msg
    assert "Custom test panic message" in err_msg


def test_subprocess_survives_rust_panic():
    """Verifies in a subprocess that a Rust panic inside FFI results in exit code 1
    (uncaught Python exception) rather than exit code 134 / SIGABRT (host process crash).
    """
    code = (
        "from memfuse import _memfuse\n"
        "try:\n"
        "    _memfuse._trigger_panic_for_test('Subprocess panic check')\n"
        "except RuntimeError as e:\n"
        "    print('EXCEPTION_CAUGHT:' + str(e))\n"
        "    import sys\n"
        "    sys.exit(0)\n"
    )
    res = subprocess.run(
        [sys.executable, "-c", code],
        capture_output=True,
        text=True,
    )

    assert res.returncode == 0, f"Subprocess failed or crashed: stderr={res.stderr}, stdout={res.stdout}"
    assert "EXCEPTION_CAUGHT:Rust panic caught at FFI boundary: Subprocess panic check" in res.stdout.strip()


def test_subprocess_uncaught_panic_exit_code():
    """Verifies that an uncaught Rust panic exception in a Python subprocess exits with code 1,
    demonstrating controlled Python unwinding rather than OS signal crash (e.g. SIGABRT exit code -6 / 134).
    """
    code = "from memfuse import _memfuse; _memfuse._trigger_panic_for_test('Uncaught panic check')"
    res = subprocess.run(
        [sys.executable, "-c", code],
        capture_output=True,
        text=True,
    )

    # Standard uncaught Python exception exits with code 1
    assert res.returncode == 1, f"Expected returncode 1, got {res.returncode}. Stderr: {res.stderr}"
    assert "RuntimeError: Rust panic caught at FFI boundary: Uncaught panic check" in res.stderr
