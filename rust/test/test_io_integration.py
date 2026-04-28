#!/usr/bin/env python3

from blobtk import io as rust_io
import io as stdlib_io
import csv
import os
try:
    import blobtk.adapters as shim_io
except Exception:
    import sys
    sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python"))
    import blobtk_io as shim_io

# Read the committed test list
lst = rust_io.read_list("test/test.list")
print("read_list:", lst)
assert isinstance(lst, list)
assert "FJNM01000076.1" in lst

# Write a small list and read it back
out_path = "test/out_list.tmp"
if os.path.exists(out_path):
    os.remove(out_path)

rust_io.write_list(["alpha", "beta"], out_path)
read_back = set(rust_io.read_list(out_path))
assert read_back == {"alpha", "beta"}
print("write/read ok")

# CSV and writer tests
tmp_txt = "test/tmp_text.txt"
if os.path.exists(tmp_txt):
    os.remove(tmp_txt)
w = rust_io.open_writer(tmp_txt)
w.write("hello\nworld\n")
w.flush()
lines = rust_io.open_lines(tmp_txt)
assert lines == ["hello", "world"]
print("text write/read ok")

tmp_csv = "test/tmp.csv"
if os.path.exists(tmp_csv):
    os.remove(tmp_csv)
rows = [["col1", "col2"], ["a", "1"], ["b", "2"]]
rust_io.write_csv(rows, tmp_csv, "\t")
read_rows = rust_io.read_csv(tmp_csv, "\t", True, None, 0, True)
assert read_rows[0] == ["a", "1"]
print("csv write/read ok")

# Iterator tests (lines + CSV)
it = rust_io.open_lines_iter("test/test.list")
collected = [x for x in it]
assert "FJNM01000076.1" in collected
print("open_lines_iter ok")

csv_it = rust_io.csv_record_iter(tmp_csv, "\t", True, None, 0, True)
csv_col = [r for r in csv_it]
assert csv_col[0] == ["a", "1"]
print("csv_record_iter ok")

# Clean up
os.remove(out_path)
os.remove(tmp_txt)
os.remove(tmp_csv)
print("io test passed")

# Additional test: use stdlib csv.writer via TextIOWrapper + shim raw IO
raw = shim_io.RustRawIO(rust_io.open_writer(tmp_csv))
text = stdlib_io.TextIOWrapper(raw, encoding="utf-8", newline="")
cw = csv.writer(text, delimiter="\t")
cw.writerows(rows)
text.flush()
text.close()
read_rows2 = rust_io.read_csv(tmp_csv, "\t", True, None, 0, True)
assert read_rows2[0] == ["a", "1"]
print("csv.writer via shim ok")
