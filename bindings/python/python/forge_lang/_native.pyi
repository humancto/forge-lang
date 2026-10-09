"""Type stubs for the compiled ``forge_lang._native`` extension."""

import os
from typing import Final, Literal, Optional, Sequence, Union, final

__version__: Final[str]
__all__ = [
    "CAPABILITIES",
    "CancelToken",
    "Diagnostic",
    "ForgeCancelledError",
    "ForgeError",
    "ForgeFuelExhaustedError",
    "ForgeMemoryLimitError",
    "ForgeOutputLimitError",
    "ForgePermissionError",
    "ForgeResourceLimitError",
    "ForgeRuntimeError",
    "ForgeSyntaxError",
    "ForgeTimeoutError",
    "Result",
    "Sandbox",
    "__version__",
    "check",
]

#: Every capability name accepted by ``Sandbox(allow=[...])``.
CAPABILITIES: Final[list[str]]

_Capability = Literal[
    "fs.read", "read", "fs.write", "write", "net", "network",
    "env", "db", "run", "ai", "process",
]
_StrPath = Union[str, "os.PathLike[str]"]

@final
class Result:
    """What a successful run produced."""

    @property
    def stdout(self) -> str:
        """Everything the program printed with ``say`` / ``println`` / ``print``."""

@final
class Diagnostic:
    """One problem reported by :func:`check`."""

    @property
    def line(self) -> int:
        """1-based line, or 0 when unknown."""
    @property
    def column(self) -> int:
        """1-based column, or 0 when unknown."""
    @property
    def is_error(self) -> bool:
        """``True`` for errors that stop the program; ``False`` for warnings."""
    @property
    def message(self) -> str: ...
    @property
    def code(self) -> Optional[str]:
        """Stable code (``E0002`` for syntax errors, ``T0006`` for type
        diagnostics; ``forge explain <code>`` documents it), when known."""
    @property
    def hint(self) -> Optional[str]:
        """How to fix it (did-you-mean, expected type), when there is a hint."""
    @property
    def severity(self) -> Literal["error", "warning"]: ...

@final
class CancelToken:
    """Stops a running program from another thread.

    Once cancelled a token stays cancelled; use a fresh token per run.
    """

    def __init__(self) -> None: ...
    def cancel(self) -> None: ...
    @property
    def cancelled(self) -> bool: ...

@final
class Sandbox:
    """A deny-by-default Forge sandbox.

    Immutable and thread-safe: configure once, then call :meth:`run` from any
    number of threads. Each run gets a fresh interpreter.

    :param allow: capabilities granted without restriction
        (``"fs.read"``, ``"fs.write"``, ``"net"``, ``"env"``, ``"db"``,
        ``"run"``, ``"ai"``, ``"process"``). Unknown names raise ``ValueError``.
    :param allow_read: grant ``fs.read`` under these directories only.
    :param allow_write: grant ``fs.write`` under these directories only.
    :param allow_net: grant ``net`` for these hosts only
        (``"example.com"``, ``"*.example.com"``, ``"host:port"``).
    :param max_time: wall-clock limit per run, in seconds.
    :param max_output: stop the program once it has printed more than this
        many bytes.
    :param max_fuel: deterministic step budget per run (one step per
        statement, function call and loop iteration).
    :param max_memory: bytes of memory one run may hold.
    :param label: name used for the source in error messages.
    """

    def __new__(
        cls,
        *,
        allow: Sequence[Union[_Capability, str]] = ...,
        allow_read: Sequence[_StrPath] = ...,
        allow_write: Sequence[_StrPath] = ...,
        allow_net: Sequence[str] = ...,
        max_time: Optional[float] = None,
        max_output: Optional[int] = None,
        max_fuel: Optional[int] = None,
        max_memory: Optional[int] = None,
        label: Optional[str] = None,
    ) -> Sandbox: ...
    def run(self, code: str, *, cancel: Optional[CancelToken] = None) -> Result:
        """Run Forge source to completion with the GIL released.

        :raises ForgeSyntaxError: the source does not parse.
        :raises ForgePermissionError: the program used an ungranted capability.
        :raises ForgeRuntimeError: any other runtime failure.
        :raises ForgeTimeoutError: ``max_time`` was exceeded.
        :raises ForgeOutputLimitError: ``max_output`` was exceeded.
        :raises ForgeCancelledError: ``cancel`` was triggered.
        :raises ForgeFuelExhaustedError: ``max_fuel`` steps were used up.
        :raises ForgeMemoryLimitError: ``max_memory`` was exceeded.
        :raises ForgeResourceLimitError: another resource cap was exceeded.
        """
    def check(self, code: str) -> list[Diagnostic]:
        """Lex, parse and type-check without running. Same as :func:`check`."""

def check(code: str) -> list[Diagnostic]:
    """Lex, parse and type-check Forge source without running it."""

class ForgeError(Exception):
    """Base class for every error raised by a Forge run."""

    #: Stable machine-readable kind: ``syntax``, ``permission_denied``,
    #: ``runtime``, ``timeout``, ``output_limit``, ``cancelled``,
    #: ``fuel_exhausted``, ``memory_limit`` or ``resource_limit``.
    kind: str
    #: Output printed before the failure (empty for syntax errors).
    stdout: str
    #: 1-based line of the failure, when known (syntax and runtime errors).
    line: Optional[int]
    #: 1-based column, when known (syntax errors).
    column: Optional[int]
    #: The limit that was hit: seconds for timeouts, bytes for output and
    #: memory limits, steps for fuel.
    limit: Optional[Union[int, float]]
    #: Stable error code (``E0012``, ...) for syntax, runtime and permission
    #: errors; ``forge explain <code>`` documents it.
    code: Optional[str]
    #: How to fix the error, when there is a hint.
    hint: Optional[str]

class ForgeSyntaxError(ForgeError): ...
class ForgePermissionError(ForgeError): ...
class ForgeRuntimeError(ForgeError): ...

class ForgeTimeoutError(ForgeError):
    limit: float

class ForgeOutputLimitError(ForgeError):
    limit: int

class ForgeCancelledError(ForgeError): ...

class ForgeFuelExhaustedError(ForgeError):
    limit: int

class ForgeMemoryLimitError(ForgeError):
    limit: int

class ForgeResourceLimitError(ForgeError): ...
