#!/usr/bin/env python3

from blobtk import io as rust_io
import os


def write_text(path, content):
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(content)


def test_fastx_iterator_and_read():
    BASE = os.path.dirname(__file__)
    fa_path = os.path.join(BASE, "tmp_test.fa")
    fq_path = os.path.join(BASE, "tmp_test.fq")

    fa_content = ">seq1\nACGT\n>seq2 description\nTTGG\n"
    fq_content = "@r1\nACGT\n+\n!!!!\n@r2\nTTGG\n+\n####\n"

    write_text(fa_path, fa_content)
    write_text(fq_path, fq_content)

    # FASTA iterator
    it = rust_io.fastx_record_iter(fa_path)
    records = [r for r in it]
    assert len(records) == 2
    assert records[0][0].startswith("seq1")
    assert records[0][1] == "ACGT"
    assert records[0][2] is None

    # FASTQ iterator
    it2 = rust_io.fastx_record_iter(fq_path)
    records2 = [r for r in it2]
    assert len(records2) == 2
    assert records2[0][0].startswith("r1")
    assert records2[0][1] == "ACGT"
    assert records2[0][2] is not None
    assert len(records2[0][2]) == len(records2[0][1])

    # read_fastx (batch)
    all_fa = rust_io.read_fastx(fa_path)
    assert len(all_fa) == 2

    # cleanup
    os.remove(fa_path)
    os.remove(fq_path)


if __name__ == "__main__":
    test_fastx_iterator_and_read()
    print("fastx test executed")
