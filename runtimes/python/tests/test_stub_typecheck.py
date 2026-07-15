"""Demonstrate the effectiveness of the generated stub through type checking itself (design §8, main battleground ②).

Run mypy per fixture as a subprocess:
- stub_ok.py = correct usage → clean (returncode 0)
- stub_bad.py = typo/type mismatch/unknown connection → "turns red the moment you write it" (returncode != 0 + an error on each line)
"""
from __future__ import annotations

import os
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[2]  # runtimes/python/tests → repository root
STUB_DIR = REPO / "examples" / "demo" / "runtime" / "gen" / "sensor"
# whole-descriptor stub (sahou_gen.py / .pyi) lives directly under the gen dir
STUB_DIR_ALL = REPO / "examples" / "demo" / "runtime" / "gen"
FIXTURES = HERE / "typestub"


def _run_mypy(fixture: str, mypypath: pathlib.Path) -> subprocess.CompletedProcess[str]:
    env = {**os.environ, "MYPYPATH": str(mypypath)}
    return subprocess.run(
        [sys.executable, "-m", "mypy", "--no-error-summary", "--cache-dir", os.devnull, str(FIXTURES / fixture)],
        capture_output=True,
        text=True,
        env=env,
        check=False,
    )


def run_mypy(fixture: str) -> subprocess.CompletedProcess[str]:
    return _run_mypy(fixture, STUB_DIR)


def run_mypy_all(fixture: str) -> subprocess.CompletedProcess[str]:
    return _run_mypy(fixture, STUB_DIR_ALL)


def test_stub_exists_as_committed_artifact():
    # the generated artifacts are committed (freshness is guarded by cli/tests/stub_freshness.rs)
    assert (STUB_DIR / "sahou_stub.py").exists()
    assert (STUB_DIR / "sahou_stub.pyi").exists()


def test_correct_usage_is_clean():
    r = run_mypy("stub_ok.py")
    assert r.returncode == 0, f"correct usage turned red:\n{r.stdout}\n{r.stderr}"


def test_typo_and_type_errors_are_red():
    r = run_mypy("stub_bad.py")
    assert r.returncode != 0, "broken usage passed clean (the stub is not working)"
    out = r.stdout
    # Because publish is a per-connection Literal overload, mypy first widens the dict-literal
    # argument as a candidate for overload resolution before matching. As a result it is
    # reported as "no overload variant matches" rather than per-key TypedDict diagnostics
    # (Extra key, etc.) (confirmed empirically; this is known overload-resolution behavior,
    # not a stub-generation bug — on the path that assigns to a typed variable, an Extra key
    # diagnostic does in fact appear, also confirmed).
    # Even so, assert that all 3 sites become type errors as independent call-overload errors.
    assert "stub_bad.py:7: error" in out, f"key typo not detected:\n{out}"        # L7: key typo (phse)
    assert "stub_bad.py:8: error" in out, f"type mismatch not detected:\n{out}"   # L8: str for x
    assert "stub_bad.py:9: error" in out, f"unknown connection not detected:\n{out}"  # L9: unknown connection (ghost)
    assert out.count('No overload variant of "publish"') == 3, (
        f"not all 3 sites become independent overload mismatches:\n{out}"
    )


def test_stub_is_not_loaded_by_engine():
    # the stub is a static-only layer: the engine package source has no reference to the stub (structural check that it is not loaded at runtime)
    engine_src = (REPO / "runtimes" / "python" / "python" / "sahou" / "_engine.py").read_text(encoding="utf-8")
    assert "sahou_stub" not in engine_src


def test_all_stub_exists_as_committed_artifact():
    # the whole-descriptor artifacts are committed (freshness is guarded by cli/tests/stub_freshness.rs)
    assert (STUB_DIR_ALL / "sahou_gen.py").exists()
    assert (STUB_DIR_ALL / "sahou_gen.pyi").exists()


def test_all_correct_usage_is_clean():
    r = run_mypy_all("all_ok.py")
    assert r.returncode == 0, f"correct usage turned red:\n{r.stdout}\n{r.stderr}"


def test_all_broken_usage_is_red():
    r = run_mypy_all("all_bad.py")
    assert r.returncode != 0, "broken usage passed clean (the whole-descriptor stub is not working)"
    out = r.stdout
    assert "all_bad.py:8: error" in out, f"unknown node name not detected:\n{out}"  # L8: node="ghost"
    assert "all_bad.py:10: error" in out, f"non-participating direction not detected:\n{out}"  # L10: VisualsNode.publish
    assert "all_bad.py:12: error" in out, f"payload type mismatch not detected:\n{out}"  # L12: x is float
    assert "all_bad.py:13: error" in out, f"unknown connection not detected:\n{out}"  # L13: conn="ghost"
