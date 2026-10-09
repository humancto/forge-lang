"""Run untrusted Forge code from Python in a deny-by-default sandbox.

>>> from forge_lang import Sandbox
>>> Sandbox().run('say "hello"').stdout
'hello\\n'

See https://github.com/humancto/forge-lang/tree/main/bindings/python
"""

from ._native import (
    CAPABILITIES,
    CancelToken,
    Diagnostic,
    ForgeCancelledError,
    ForgeError,
    ForgeFuelExhaustedError,
    ForgeMemoryLimitError,
    ForgeOutputLimitError,
    ForgePermissionError,
    ForgeResourceLimitError,
    ForgeRuntimeError,
    ForgeSyntaxError,
    ForgeTimeoutError,
    Result,
    Sandbox,
    __version__,
    check,
)

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
