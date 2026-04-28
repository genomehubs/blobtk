# BlobTK Rust <-> Python IO

This document describes the Python-facing IO utilities exposed by the Rust extension and the small pure-Python adapters shipped in the wheel.

## Package layout

- The native extension is built as `blobtk._blobtk` and re-exported as the `blobtk` package when installed from the `rust/` tree.
- The pure-Python helpers live in `blobtk.adapters` (see `rust/blobtk/adapters.py`).

## Primary Rust functions exposed to Python

These functions are available on the `blobtk.io` namespace (import via `from blobtk import io as rust_io`):

- `read_list(path: str) -> list[str]`
  Read a simple newline-delimited list file and return a Python `list` of strings.

- `write_list(items: list[str], path: str)`
  Write a small list of strings (one-per-line) to `path`.

- `open_lines(path: str) -> list[str]`
  Read a text file and return a list of lines (trimmed).

- `open_lines_iter(path: str)`
  Return a Python iterator that yields lines lazily from `path`.

- `open_writer(path: str)` / `open_append_writer(path: str)`
  Return a `PyWriter` object (Rust-backed) that implements write/flush/close and convenience methods described below. Use `open_append_writer` to open in append mode.

- `write_text(text: str, path: str)` / `append_text(text: str, path: str)`
  Convenience helpers to write/append full text blobs.

- `read_csv(path: str, sep: str, has_header: bool, quotechar: Optional[str], skip_rows: int, trim: bool) -> list[list[str]]`
  Read a delimited CSV/TSV file into a list of rows.

- `write_csv(rows: Iterable[Iterable[str]], path: str, sep: str)`
  Write rows to a CSV/TSV file using the given delimiter.

- `csv_record_iter(path, sep, has_header, quotechar, skip_rows, trim)`
  Stream CSV records as an iterator (yields lists of strings).

## `PyWriter` methods

`open_writer()` and `open_append_writer()` return a `PyWriter` object implemented in Rust. Important methods:

- `write(s: str) -> int` — write a text string (returns bytes written)
- `write_bytes(b: bytes) -> int` — write raw bytes
- `writelines(iterable)` — write multiple lines
- `flush()` — flush buffered content
- `close()` — close the writer and underlying resource
- `fileno() -> Optional[int]` — return the OS file descriptor if available (Unix). Returns `None` where not applicable.
- `detach() -> int` — Unix-only: consume Python wrapper ownership and return the raw fd (caller becomes owner).
- Context-manager support: `with rust_io.open_writer(path) as w:` works as expected.

## Notes about `fileno()` and `detach()`

- `fileno()` exposes the underlying FD when the writer is backed by a real file. It may return `None` for non-file-backed writers (e.g., gzip streams on some platforms).
- `detach()` transfers ownership of the underlying file descriptor to the caller (Unix only). After `detach()` the writer may not be usable; callers should follow POSIX rules (close the fd when done).

## Adapters: `blobtk.adapters`

The package exposes a small adapter module to make the Rust `PyWriter` usable with Python stdlib `io` APIs:

- `RustRawIO` — a `io.RawIOBase` implementation that delegates to a `PyWriter`.
- `open_text_writer(inner, encoding='utf-8', newline='')` — convenience factory returning a `io.TextIOWrapper` around a `RustRawIO` (ready to use with `csv.writer`, etc.).

## Usage examples

Write text + use `fileno()`:

```py
from blobtk import io as rust_io
from blobtk import adapters

w = rust_io.open_writer('out.txt')
w.write('hello\n')
print('fd:', w.fileno())
w.flush()
w.close()
```

Use `TextIOWrapper` and `csv.writer` via the adapter:

```py
from blobtk import io as rust_io
from blobtk.adapters import open_text_writer
import csv

raw = rust_io.open_writer('out.csv')
text = open_text_writer(raw)
cw = csv.writer(text, delimiter='\t')
cw.writerow(['a','1'])
text.flush()
text.close()
```

## Testing and CI

- The repository includes tests under `rust/test/` that exercise the adapter and `detach()` behaviour. CI runs these tests against built wheels.

## Platform notes

- `detach()` is Unix-specific and will raise on unsupported platforms.
- `fileno()` may return `None` for some stream types; callers should handle that case.
