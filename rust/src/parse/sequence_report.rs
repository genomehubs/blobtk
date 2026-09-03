//! Parse JSON or TSV format sequence reports
//!
//! The sequence report is a JSON or TSV file that contains information about the sequences in a dataset, including their attributes and metadata. This module provides functions to parse the sequence report and extract the relevant information for indexing into Elasticsearch.
//! JSON format reports can be obtained via NCBI datsets API, while TSV format reports can be obtained via NCBI E-utilities API or FTP. The module includes functions to handle both formats and convert them into a common internal representation for further processing and indexing.
//!

use std::collections::HashMap;
use std::fs::{create_dir_all, write};
use std::io::BufRead;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{self, Error};
use crate::import::SequenceReportImportConfig;
use crate::index::es::models::documents::FeatureDocument;
use crate::index::es::models::nested_documents::NestedAttribute;
use crate::io;
use crate::parse::genomehubs::StringOrVec;

#[derive(Debug, Deserialize, Serialize)]
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
        let start = 1;
        let end = self.length;
        let strand = Some(1);
        let sequence_length = self.length;
        let gc = self.gc_percent.map(|gc_percent| gc_percent / 100.0);
        let mut feature_names = vec![];
        if let Some(sequence_name) = &self.sequence_name {
            feature_names.push(sequence_name.clone());
        }
        let mut attributes = vec![
            NestedAttribute {
                key: "sequence_id".to_string(),
                keyword_value: Some(StringOrVec::Single(sequence_id.clone())),
                ..Default::default()
            },
            NestedAttribute {
                key: "assembly_id".to_string(),
                keyword_value: Some(StringOrVec::Single(assembly_id.clone())),
                ..Default::default()
            },
            NestedAttribute {
                key: "taxon_id".to_string(),
                keyword_value: Some(StringOrVec::Single(taxon_id.clone())),
                ..Default::default()
            },
        ];
        if let Some(chr_name) = &self.chr_name {
            if self.role == "assembled-molecule" {
                feature_names.push(chr_name.clone());
                attributes.push(NestedAttribute {
                    key: "chromosome_name".to_string(),
                    keyword_value: Some(StringOrVec::Single(chr_name.clone())),
                    ..Default::default()
                });
            }
        }
        if let Some(gc) = gc {
            attributes.push(NestedAttribute {
                key: "gc".to_string(),
                half_float_value: Some(gc as f32),
                ..Default::default()
            });
        }
        if feature_names.len() > 0 {
            attributes.push(NestedAttribute {
                key: "feature_name".to_string(),
                keyword_value: Some(StringOrVec::Multiple(feature_names)),
                ..Default::default()
            });
        }
        let mut feature_doc = FeatureDocument::new(
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
            Some(vec![]),
            None, // file_id
            None, // analysis_id
        );
        // add attributes to feature_doc
        if let Some(feature_attrs) = feature_doc.attributes.as_mut() {
            for attr in attributes {
                feature_attrs.push(attr);
            }
        }
        feature_doc
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
    ancestor_taxon_ids: Vec<String>,
) -> Result<HashMap<String, FeatureDocument>, error::Error> {
    let mut feature_docs: HashMap<String, FeatureDocument> = HashMap::new();
    for line in json_lines.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let record: DatasetsSequenceReport = serde_json::from_str(line)
            .map_err(|e| Error::Generic(format!("Failed to parse JSON line: {}", e)))?;
        let mut feature_doc = record.to_feature(taxon_id.clone());
        feature_doc.ancestors = Some(ancestor_taxon_ids.clone());
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
    if let Some(p) = config.local_path {
        let maybe_sr_file = io::file_reader(p.clone());
        if let Ok(mut sr_file) = maybe_sr_file {
            let sr_reader = &mut *sr_file;
            let json_lines: String = sr_reader
                .lines()
                .map(|line| line.unwrap_or_default())
                .collect::<Vec<String>>()
                .join("\n");
            return parse_sequence_report_from_json_lines(
                &json_lines,
                config.taxon_id,
                config.ancestors,
            );
        }
        // path was provided but not present locally -> fetch and cache
        let json_lines = fetch_datasets_sequence_report(&config.accession)?;
        let mut writer = io::get_writer(&Some(p.clone()));
        writer.write_all(json_lines.as_bytes())?;
        return parse_sequence_report_from_json_lines(
            &json_lines,
            config.taxon_id,
            config.ancestors,
        );
    }

    // No path given -> fetch but don't cache
    let json_lines = fetch_datasets_sequence_report(&config.accession)?;
    parse_sequence_report_from_json_lines(&json_lines, config.taxon_id, config.ancestors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_report_feature_includes_sequence_aliases_in_feature_type() {
        let record = DatasetsSequenceReport {
            assembly_accession: "GCA_test".to_string(),
            assigned_molecule_location_type: "chromosome".to_string(),
            chr_name: Some("chr1".to_string()),
            gc_percent: Some(41.2),
            genbank_accession: "CM000001.1".to_string(),
            length: 1_000_000,
            role: "assembled-molecule".to_string(),
            sequence_name: Some("chr1".to_string()),
        };

        let feature = record.to_feature("1234".to_string());

        assert_eq!(feature.primary_type, "chromosome");
        assert!(feature
            .attributes
            .as_ref()
            .unwrap()
            .iter()
            .any(|attr| attr.key == "feature_type"
                && matches!(
                    attr.keyword_value.as_ref(),
                    Some(crate::parse::genomehubs::StringOrVec::Multiple(values))
                        if values.iter().any(|v| v == "sequence")
                            && values.iter().any(|v| v == "nuclear-sequence")
                            && values.iter().any(|v| v == "toplevel")
                )));
    }
}
