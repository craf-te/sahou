"""Diagnostics and exceptions. A diagnostic is always {code, path, message} (from the core, byte-identical across the 3 languages)."""
from __future__ import annotations

import json
from dataclasses import dataclass


@dataclass(frozen=True)
class Diag:
    code: str
    path: str
    message: str

    @staticmethod
    def from_dict(d: dict) -> "Diag":
        return Diag(d["code"], d["path"], d["message"])

    def __str__(self) -> str:
        return f"[{self.code}] @{self.path}: {self.message}"


def diags_from_json(s: str) -> list[Diag]:
    return [Diag.from_dict(d) for d in json.loads(s)]


class SahouError(Exception):
    """Base exception for Sahou."""


class SahouRejected(SahouError):
    """A NO at a boundary (send boundary, or a fatal delivery failure). Structured diagnostics in diags."""

    def __init__(self, diags: list[Diag] | list[dict]):
        self.diags = [d if isinstance(d, Diag) else Diag.from_dict(d) for d in diags]
        super().__init__("; ".join(str(d) for d in self.diags))


class SahouUnreachable(SahouError):
    """No response even after exhausting retries (possibly a transient failure; already retried)."""

    def __init__(self, conn: str, attempts: int):
        self.conn = conn
        self.attempts = attempts
        super().__init__(f"no response from connection '{conn}' ({attempts} attempts)")
