use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;

extern crate needletail;
use needletail::parser::{write_fasta, LineEnding};
use needletail::FastxReader;

use crate::fastq::{open_fastx, suffix_file_name};
use crate::io::get_writer;
use crate::utils::styled_progress_bar;

fn trim_seq_id(input: &[u8]) -> Vec<u8> {
    input
        .iter()
        .copied()
        .by_ref()
        .take_while(|&x| x != b' ')
        .collect()
}

fn subsample_fasta<F: Fn()>(
    seq_names: &HashSet<Vec<u8>>,
    mut reader: Box<dyn FastxReader>,
    writer: &mut dyn Write,
    callback: &Option<F>,
) {
    let total = seq_names.len();
    let progress_bar = styled_progress_bar(total, "Subsampling FASTA");

    while let Some(record) = reader.next() {
        let seqrec = record.as_ref().expect("invalid record");
        let seq_id: Vec<u8> = trim_seq_id(seqrec.id());
        if seq_names.contains(&seq_id) {
            write_fasta(seqrec.id(), &seqrec.seq(), writer, LineEnding::Unix)
                .expect("Unable to write FASTA");
            progress_bar.inc(1);
            if progress_bar.position() as usize == total {
                break;
            }
        }

        if let Some(cb) = callback {
            cb()
        }
    }
    progress_bar.finish();
}

pub fn subsample<F: Fn()>(
    seq_names: &HashSet<Vec<u8>>,
    fasta_path: &Option<PathBuf>,
    fasta_out: &bool,
    suffix: &String,
    callback: &Option<F>,
) {
    if fasta_path.is_none() {
        return;
    }
    if !fasta_out {
        return;
    }

    let reader = open_fastx(fasta_path);
    let out_path = suffix_file_name(fasta_path.as_ref().unwrap(), suffix);
    let mut writer = get_writer(&Some(out_path));

    if let Some(r) = reader {
        subsample_fasta(seq_names, r, &mut *writer, callback);
    }
}
