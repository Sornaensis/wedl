from __future__ import annotations

from typing import Any


class WedlError(Exception):
    code = "wedl_error"

    def __init__(self, message: str, *, details: dict[str, Any] | None = None) -> None:
        super().__init__(message)
        self.message = message
        self.details = details or {}

    def as_dict(self) -> dict[str, Any]:
        return {"code": self.code, "message": self.message, "details": self.details}


class UsageError(WedlError):
    code = "usage_error"


class CompileRequired(WedlError):
    """A read was explicitly configured not to compile an unavailable cache."""

    code = "compile_required"


class RepositoryError(WedlError):
    code = "repository_error"


class ServeError(WedlError):
    """The local web server could not be prepared or announced safely."""

    code = "serve_error"


class ParseError(WedlError):
    code = "parse_error"


class SupersededSchemaError(WedlError):
    """A withdrawn source schema must be recovered outside normal loading."""

    code = "v04_superseded"


class ValidationFailed(WedlError):
    code = "validation_failed"

    def __init__(self, message: str, diagnostics: list[dict[str, Any]]) -> None:
        super().__init__(message, details={"diagnostics": diagnostics})
        self.diagnostics = diagnostics


class NotFound(WedlError):
    code = "not_found"


class ConflictError(WedlError):
    code = "conflict"


class StaleRevision(WedlError):
    code = "stale_revision"


class ConfirmationRequired(WedlError):
    """A mutation needs the proof token emitted by a successful preview."""

    code = "confirmation_required"


class ConfirmationMismatch(WedlError):
    """The supplied proof token does not describe this request and revision."""

    code = "confirmation_mismatch"


class DirtyManagedTree(WedlError):
    code = "dirty_managed_tree"
