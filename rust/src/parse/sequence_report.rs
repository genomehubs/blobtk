//! Parse JSON or TSV format sequence reports
//!
//! The sequence report is a JSON or TSV file that contains information about the sequences in a dataset, including their attributes and metadata. This module provides functions to parse the sequence report and extract the relevant information for indexing into Elasticsearch.
//! JSON format reports can be obtained via NCBI datsets API, while TSV format reports can be obtained via NCBI E-utilities API or FTP. The module includes functions to handle both formats and convert them into a common internal representation for further processing and indexing.
//!
//! The module uses the Feature struct to represent the attributes and metadata of each sequence, and provides functions to parse the JSON or TSV report and extract the relevant information into a vector of Feature structs. The module also includes error handling to ensure that any issues encountered during parsing are properly reported and handled.
//!
//! Example usage:
//! ```rust
//! use crate::parse::sequence_report::{parse_json_report, parse_tsv_report};
//! let json_report = std::fs::read_to_string("sequence_report.json").unwrap();
//! let tsv_report = std::fs::read_to_string("sequence_report.tsv").unwrap();
//! let sequences_from_json = parse_json_report(&json_report).unwrap();
//! let sequences_from_tsv = parse_tsv_report(&tsv_report).unwrap();
//! ```
