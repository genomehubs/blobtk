//! `blobtk` is a set of core command line utilities and python bindings for
//! processing common file formats used by [BlobToolKit](https://blobtoolkit.genomehubs.org).

/// Functions for processing BAM files.
pub mod bam;

/// Functions for processing a BlobDir.
pub mod blobdir;

/// The BlobTk Command Line Interface.
pub mod cli;

/// Summarise windowed coverage depth.
pub mod depth;

/// Error handline.
pub mod error;

/// Functions for processing FASTA files.
pub mod fasta;

/// Functions for processing FASTQ files.
pub mod fastq;

/// Filter files based on a list of sequence IDs.
pub mod filter;

/// Index files for a GenomeHubs instance.
pub mod index;

/// Functions for file/terminal IO.
pub mod io;

/// Functions for parsing files
pub mod parse;

/// Generate a plot.
pub mod plot;

/// Python bindings.
#[cfg(feature = "python-extension")]
pub mod python;

/// Parse and subset a taxonomy.
pub mod taxonomy;

/// Utility functions.
pub mod utils;

/// Validate files.
pub mod validate;
