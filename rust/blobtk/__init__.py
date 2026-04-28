import importlib
import sys

# Import the compiled extension module created by maturin as a submodule
_ext = importlib.import_module("blobtk._blobtk")

# Re-export public symbols from the extension into the package namespace
for _name in dir(_ext):
    if not _name.startswith("_"):
        globals()[_name] = getattr(_ext, _name)

# Expose the extension's `io` submodule as `blobtk.io` for compatibility
try:
    _io_mod = importlib.import_module("blobtk._blobtk.io")
    sys.modules["blobtk.io"] = _io_mod
    io = _io_mod
except ModuleNotFoundError:
    io = getattr(_ext, "io", None)

__all__ = [k for k in globals().keys() if not k.startswith("_")]
