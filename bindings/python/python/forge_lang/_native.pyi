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
    "ForgeOutputLimitError",
    "ForgePermissionError",
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
    :param label: name used for the source in error messages.
    :param engine: ``"vm"`` (default, the bytecode VM) or ``"interp"`` (the
        tree-walking interpreter). Both run under the same sandbox.
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
        label: Optional[str] = None,
        engine: Optional[Literal["vm", "interp"]] = None,
    ) -> Sandbox: ...
    def run(self, code: str, *, cancel: Optional[CancelToken] = None) -> Result:
        """Run Forge source to completion with the GIL released.

        :raises ForgeSyntaxError: the source does not parse.
        :raises ForgePermissionError: the program used an ungranted capability.
        :raises ForgeRuntimeError: any other runtime failure.
        :raises ForgeTimeoutError: ``max_time`` was exceeded.
        :raises ForgeOutputLimitError: ``max_output`` was exceeded.
        :raises ForgeCancelledError: ``cancel`` was triggered.
        """
    def check(self, code: str) -> list[Diagnostic]:
        """Lex, parse and type-check without running. Same as :func:`check`."""

def check(code: str) -> list[Diagnostic]:
    """Lex, parse and type-check Forge source without running it."""

class ForgeError(Exception):
    """Base class for every error raised by a Forge run."""

    #: Stable machine-readable kind: ``syntax``, ``permission_denied``,
    #: ``runtime``, ``timeout``, ``output_limit`` or ``cancelled``.
    kind: str
    #: Output printed before the failure (empty for syntax errors).
    stdout: str
    #: 1-based line of the failure, when known (syntax and runtime errors).
    line: Optional[int]
    #: 1-based column, when known (syntax errors).
    column: Optional[int]
    #: The limit that was hit: seconds for timeouts, bytes for output limits.
    limit: Optional[Union[int, float]]

class ForgeSyntaxError(ForgeError): ...
class ForgePermissionError(ForgeError): ...
class ForgeRuntimeError(ForgeError): ...

class ForgeTimeoutError(ForgeError):
    limit: float

class ForgeOutputLimitError(ForgeError):
    limit: int

class ForgeCancelledError(ForgeError): ...
