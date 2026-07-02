//! Parse JSON or TSV format sequence reports
//!
//! The sequence report is a JSON or TSV file that contains information about the sequences in a dataset, including their attributes and metadata. This module provides functions to parse the sequence report and extract the relevant information for indexing into Elasticsearch.
//! JSON format reports can be obtained via NCBI datsets API, while TSV format reports can be obtained via NCBI E-utilities API or FTP. The module includes functions to handle both formats and convert them into a common internal representation for further processing and indexing.
//!

use std::collections::HashMap;
use std::fs::{create_dir_all, write, File};
use std::io::BufRead;
use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{self, Error};
use crate::import::SequenceReportImportConfig;
use crate::index::es::config;
use crate::index::es::models::documents::FeatureDocument;
use crate::index::es::models::nested_documents::NestedAttribute;
use crate::io;

#[derive(Debug, Deserialize)]
pub struct DatasetsSequenceReport {
    assembly_accession: String,
    // assembly_unit: String,
    assigned_molecule_location_type: String,
    chr_name: Option<String>,
    // gc_count: String,
    gc_percent: Option<f64>,
    genbank_accession: String,
    length: usize,
    role: String,
    sequence_name: Option<String>,
}

impl DatasetsSequenceReport {
    pub fn to_feature(&self, taxon_id: String) -> FeatureDocument {
        let feature_id = self.genbank_accession.clone();
        let sequence_id = self.genbank_accession.clone();
        let assembly_id = self.assembly_accession.clone();
        let primary_type = match self.role.as_str() {
            "assembled-molecule" => self
                .assigned_molecule_location_type
                .to_string()
                .to_lowercase(),
            "unplaced-scaffold" => "scaffold".to_string(),
            "unlocalized-scaffold" => "scaffold".to_string(),
            "unlocalized-contig" => "contig".to_string(),
            _ => "contig".to_string(),
        };
        let feature_type = vec![
            primary_type.clone(),
            "sequence".to_string(),
            "toplevel".to_string(),
        ];
        let start = 1;
        let end = self.length;
        let strand = Some(1);
        let sequence_length = self.length;
        let gc = self.gc_percent.map(|gc_percent| gc_percent / 100.0);
        let midpoint = sequence_length / 2;
        let midpoint_proportion = 0.5;
        let seq_proportion = 1.0;
        let mut names = vec![];
        if let Some(sequence_name) = &self.sequence_name {
            names.push(sequence_name.clone());
        }
        if let Some(chr_name) = &self.chr_name {
            names.push(chr_name.clone());
        }
        let name = if !names.is_empty() {
            Some(names.join(",").to_string())
        } else {
            None
        };
        FeatureDocument::new(
            feature_id,
            None,
            primary_type,
            start,
            end,
            strand, // strand
            None,   // container_ids
            sequence_id,
            sequence_length,
            assembly_id,
            taxon_id,
            None, // ancestors
            None, // file_id
            None, // analysis_id
        )
    }
}

// Fetch JSON-lines from NCBI `datasets` for the given accession.
// Returns the raw stdout (JSON lines) as String.
fn fetch_datasets_sequence_report(accession: &str) -> Result<String, error::Error> {
    if Command::new("datasets").output().is_err() {
        return Err(Error::Generic(
            "datasets command not found. Please install NCBI datasets command line tool."
                .to_string(),
        ));
    }

    let output = Command::new("datasets")
        .args([
            "summary",
            "genome",
            "accession",
            accession,
            "--report",
            "sequence",
            "--as-json-lines",
        ])
        .output()?;

    if !output.status.success() {
        return Err(Error::Generic(format!(
            "datasets command failed with status: {}",
            output.status
        )));
    }

    let json_lines = String::from_utf8(output.stdout)?;
    Ok(json_lines)
}

// Parse JSON-lines (as produced by datasets) into FeatureDocument map.
fn parse_sequence_report_from_json_lines(
    json_lines: &str,
    taxon_id: String,
) -> Result<HashMap<String, FeatureDocument>, error::Error> {
    let mut feature_docs: HashMap<String, FeatureDocument> = HashMap::new();
    for line in json_lines.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let record: DatasetsSequenceReport = serde_json::from_str(line)
            .map_err(|e| Error::Generic(format!("Failed to parse JSON line: {}", e)))?;
        let feature_doc = record.to_feature(taxon_id.clone());
        feature_docs.insert(feature_doc.feature_id.clone(), feature_doc);
    }
    Ok(feature_docs)
}

/// Wrapper: read local `path` if it exists; otherwise fetch via `datasets`.
/// If `path` is provided but doesn't exist, fetch and write the file so it's cached.
pub fn parse_sequence_report(
    config: SequenceReportImportConfig,
) -> Result<HashMap<String, FeatureDocument>, error::Error> {
    // If a path is provided and exists, read from it.
    if let Some(p) = config.path {
        let maybe_sr_file = io::file_reader(p.clone());
        if let Ok(mut sr_file) = maybe_sr_file {
            let sr_reader = &mut *sr_file;
            let json_lines: String = sr_reader
                .lines()
                .map(|line| line.unwrap_or_default())
                .collect::<Vec<String>>()
                .join("\n");
            return parse_sequence_report_from_json_lines(&json_lines, config.taxon_id);
        }
        // path was provided but not present locally -> fetch and cache
        let json_lines = fetch_datasets_sequence_report(&config.accession)?;
        if let Some(parent) = p.parent() {
            create_dir_all(parent)?; // ensure parent exists
        }
        write(&p, &json_lines)?; // write cache
        return parse_sequence_report_from_json_lines(&json_lines, config.taxon_id);
    }

    // No path given -> fetch but don't cache
    let json_lines = fetch_datasets_sequence_report(&config.accession)?;
    parse_sequence_report_from_json_lines(&json_lines, config.taxon_id)
}
