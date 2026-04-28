import io as _io


class RustRawIO(_io.RawIOBase):
    """Wrap a Rust `PyWriter` returned by `blobtk.io.open_writer` as a RawIOBase.

    The underlying Rust object must implement `write_bytes(data: bytes)`,
    `flush()` and `close()` (these are provided by the `PyWriter` class).
    """

    def __init__(self, inner):
        self._inner = inner
        self._closed = False

    def writable(self):
        return True

    def write(self, b):
        # Accept str or bytes-like; TextIOWrapper will pass bytes
        if isinstance(b, str):
            b = b.encode("utf-8")
        # Delegate to Rust write_bytes; it accepts bytes/bytearray
        n = self._inner.write_bytes(b)
        return int(n)

    def flush(self):
        self._inner.flush()

    def close(self):
        if not self._closed:
            try:
                self._inner.flush()
            except Exception:
                pass
            try:
                self._inner.close()
            except Exception:
                pass
            self._closed = True

    @property
    def closed(self):
        return self._closed

    def fileno(self):
        # Return underlying fileno if available, else raise OSError
        try:
            fd = self._inner.fileno()
        except Exception:
            raise OSError("fileno unavailable")
        if fd is None:
            raise OSError("fileno unavailable")
        return int(fd)


def open_text_writer(inner, encoding="utf-8", newline=""):
    raw = RustRawIO(inner)
    return _io.TextIOWrapper(raw, encoding=encoding, newline=newline)
