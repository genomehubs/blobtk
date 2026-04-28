<!-- Short README for Python FASTX helpers (FASTA / FASTQ) -->

# Python — FASTX helpers (FASTA / FASTQ)

This short reference describes the Python-facing FASTA/FASTQ helpers exposed by the Rust extension (available under `blobtk.io`) and how to use them from downstream code.

Quick summary

- `fastx_record_iter(path: Optional[str]) -> Iterator[(id, seq, qual_or_None)]` — streaming iterator for FASTA and FASTQ. Yields tuples `(id: str, sequence: str, quality: Optional[str])`. For FASTA files `quality` is `None`.
- `read_fastx(path: Optional[str]) -> List[(id, seq, qual_or_None)]` — convenience function that reads all records into memory and returns a list of tuples.

When to use which

- Use `fastx_record_iter()` for large files or streaming pipelines where you want to process records one-by-one and avoid loading the entire file into memory.
- Use `read_fastx()` for small inputs, tests, or quick scripts where simplicity outweighs memory use.

Example — streaming processing

```python
from blobtk import io as rust_io

# process sequences one at a time
for rec_id, seq, qual in rust_io.fastx_record_iter('assembly.fa'):
    # rec_id may include description text after whitespace
    name = rec_id.split()[0]
    # process `seq` here; `qual` is None for FASTA
    print(name, len(seq))
```

Example — quick read

```python
from blobtk import io as rust_io

records = rust_io.read_fastx('reads.fq')
for rec_id, seq, qual in records:
    # use seq and qual (qual is a str for FASTQ)
    pass
```

Notes

- These functions reuse the Rust `open_fastx` helper, so input transparently supports gzipped files (and other sources handled by the project's `file_reader`).
- `fastx_record_iter()` returns native Python strings for `id`, `sequence`, and `quality` (when present). Quality strings match sequence length for FASTQ records.
- There is a pytest integration test at `rust/test/test_fastx_iter.py` demonstrating both FASTA and FASTQ usage.
