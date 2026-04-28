#!/usr/bin/env python3

import os
from blobtk import io as rust_io


def test_detach_roundtrip():
    tmp = "test/tmp_detach.txt"
    w = None
    try:
        if os.path.exists(tmp):
            os.remove(tmp)

        w = rust_io.open_writer(tmp)
        fd = w.detach()
        assert isinstance(fd, int) and fd >= 0

        # write raw bytes via the detached fd
        os.write(fd, b"raw\n")
        os.close(fd)

        # write via the Rust writer and flush
        w.write("via_writer\n")
        w.flush()

        with open(tmp, "rb") as f:
            content = f.read()

        assert b"raw\n" in content
        assert b"via_writer\n" in content
    finally:
        try:
            if w is not None:
                w.close()
        except Exception:
            pass
        try:
            if os.path.exists(tmp):
                os.remove(tmp)
        except Exception:
            pass
