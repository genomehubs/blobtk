//! Functions to parse a busco full table file and return an iterator of BuscoFeature structs.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;

use anyhow;
use serde::{Deserialize, Serialize};

use crate::error;
use crate::import::state::ImportState;
use crate::import::sync_attribute_documents;
use crate::import::EsConfig;
use crate::import::ImportOptions;
use crate::index::es::client::ElasticsearchClient;
use crate::index::es::models::attribute_builder::{
    build_attribute_document, AttributeDocOverrides,
};
use crate::index::es::models::documents::FeatureDocument;
use crate::index::es::models::nested_documents::NestedAttribute;
use crate::io::get_csv_reader;
use crate::parse::bed::WindowSpec;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BuscoTableConfig {
    pub path: PathBuf,
    #[serde(default)]
    pub lineages: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct BuscoFileConfig {
    pub path: PathBuf,
    pub lineage: String,
    pub taxon_id: String,
    pub accession: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AlgConfig {
    pub name: String,
    pub lineage: String,
    pub path: String,
    pub mapping: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct MultiBuscoConfig {
    pub accession: String,
    pub taxon_id: String,
    #[serde(skip_serializing)]
    pub tables: Option<Vec<BuscoTableConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<BuscoFileConfig>>,
    pub algs: Option<Vec<AlgConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuscoFeature {
    pub id: String,
    pub status: String,
    pub score: f64,
    pub sequence: String,
    pub start: usize,
    pub end: usize,
    pub strand: i8,
    pub length: usize,
}

#[derive(Debug, Clone)]
pub struct ParsedBuscoLine {
    pub id: String,
    pub feature: Option<BuscoFeature>, // None if incomplete/missing
}

fn parse_full_table(
    mut full_table_reader: csv::Reader<Box<dyn BufRead>>,
) -> impl Iterator<Item = Result<ParsedBuscoLine, anyhow::Error>> {
    let headers = full_table_reader.headers().unwrap().clone();
    let id_index = headers.iter().position(|h| h == "# Busco id").unwrap();
    let status = headers.iter().position(|h| h == "Status").unwrap();
    let sequence_index = headers.iter().position(|h| h == "Sequence").unwrap();
    let gene_start_index = headers.iter().position(|h| h == "Gene Start").unwrap();
    let gene_end_index = headers.iter().position(|h| h == "Gene End").unwrap();
    let strand_index = headers.iter().position(|h| h == "Strand").unwrap();
    let score_index = headers.iter().position(|h| h == "Score").unwrap();
    let length_index = headers.iter().position(|h| h == "Length").unwrap();

    full_table_reader.into_records().map(move |result| {
        if let Ok(record) = result {
            let id = record.get(id_index).unwrap().to_string();

            // Try to parse the full feature; if incomplete, return with id only
            if record.len() < 8 {
                return Ok(ParsedBuscoLine {
                    id,
                    feature: None, // Incomplete row - missing BUSCO
                });
            }

            let status = record.get(status).unwrap().to_string();
            if record.len() < 8 {
                return Err(anyhow::anyhow!("{}: {}", id, status));
            }
            let sequence = record.get(sequence_index).unwrap().to_string();
            let start: usize = record.get(gene_start_index).unwrap().parse()?;
            let end: usize = record.get(gene_end_index).unwrap().parse()?;
            let strand: i8 = match record.get(strand_index).unwrap() {
                "+" => 1,
                "-" => -1,
                _ => return Err(anyhow::anyhow!("Invalid strand value")),
            };
            let score: f64 = record.get(score_index).unwrap().parse()?;
            let length: usize = record.get(length_index).unwrap().parse()?;
            Ok(ParsedBuscoLine {
                id: id.clone(),
                feature: Some(BuscoFeature {
                    id,
                    status,
                    score,
                    sequence,
                    start,
                    end,
                    strand,
                    length,
                }),
            })
        } else {
            Err(anyhow::anyhow!("Error reading record"))
        }
    })
}

pub fn parse_alg_files(
    alg_configs: Vec<AlgConfig>,
) -> Result<HashMap<String, AlgConfig>, anyhow::Error> {
    let mut alg_map = HashMap::new();
    for alg in alg_configs {
        let alg_reader = get_csv_reader(
            &Some(PathBuf::from(alg.path.clone())),
            b'\t',
            false,
            None,
            0,
            true,
        )?;
        let mut mapping = HashMap::new();
        for result in alg_reader.into_records() {
            let record = result?;
            let key = record.get(0).unwrap().to_string();
            let value = record.get(1).unwrap().to_string();
            mapping.insert(key, value);
        }
        alg_map.insert(
            alg.name.clone(),
            AlgConfig {
                name: alg.name.clone(),
                path: alg.path.clone(),
                lineage: alg.lineage.clone(),
                mapping: Some(mapping),
            },
        );
    }
    Ok(alg_map)
}

fn overlapping_window_ids(
    sequence_id: &str,
    sequence_length: usize,
    feat_start_1based: usize,
    feat_end_1based: usize,
    window_specs: &[WindowSpec],
    lines_per_unit: usize,
) -> Vec<String> {
    let s = feat_start_1based.saturating_sub(1);
    let e = feat_end_1based.max(feat_start_1based); // guard malformed input

    let mut ids = Vec::new();

    for spec in window_specs {
        let mut window_bp = match spec {
            WindowSpec::Size { size: bp } => *bp,
            WindowSpec::Proportion { proportion: p } => {
                ((sequence_length as f64) * *p).ceil() as usize
            }
        };

        if window_bp == 0 {
            continue;
        }

        // Optional: match BED quantization to 1kb units
        if lines_per_unit > 1 {
            window_bp = window_bp.div_ceil(lines_per_unit) * lines_per_unit;
        }

        let first = s / window_bp;
        let last = e.saturating_sub(1) / window_bp;

        for i in first..=last {
            let w_start = i * window_bp;
            let w_end = ((i + 1) * window_bp).min(sequence_length);
            ids.push(crate::parse::bed::set_window_name(
                sequence_id,
                w_start,
                w_end,
                spec,
            ));
        }
    }

    ids
}

pub struct BuscoIdTracker {
    // busco_id -> Vec<(sequence_id, window_ids, status, lineage)>
    pub occurrences: HashMap<String, Vec<(String, Vec<String>, String, String)>>,
}

impl BuscoIdTracker {
    pub fn new() -> Self {
        BuscoIdTracker {
            occurrences: HashMap::new(),
        }
    }

    pub fn record(
        &mut self,
        busco_id: &str,
        sequence_id: &str,
        window_ids: Vec<String>,
        status: &str,
        lineage: &str, // NEW
    ) {
        self.occurrences
            .entry(busco_id.to_string())
            .or_insert_with(Vec::new)
            .push((
                sequence_id.to_string(),
                window_ids,
                status.to_string(),
                lineage.to_string(),
            ));
    }

    pub fn categorize(&self, busco_id: &str, lineage: &str) -> Vec<String> {
        if let Some(occurrences) = self.occurrences.get(busco_id) {
            let mut categories = Vec::new();

            // Check for Complete or Duplicated statuses (case-insensitive)
            let has_complete_or_duplicated = occurrences.iter().any(|(_, _, status, _)| {
                status.to_lowercase() == "complete" || status.to_lowercase() == "duplicated"
            });

            let has_missing = occurrences
                .iter()
                .any(|(_, _, status, _)| status.to_lowercase() == "missing");

            let total_occurrences = occurrences.len();

            if has_complete_or_duplicated {
                categories.push("complete".to_string());
                if total_occurrences > 1 {
                    categories.push("duplicated".to_string());
                } else {
                    categories.push("single_copy".to_string());
                }
            }

            if has_missing {
                categories.push("missing".to_string());
            }

            if categories.is_empty() {
                categories.push("fragmented".to_string());
            }

            categories
        } else {
            vec![format!("{}_unknown", lineage)]
        }
    }
}

pub fn parse_busco_files(
    busco_config: &MultiBuscoConfig,
    sequence_features: &HashMap<String, FeatureDocument>,
    window_cfg: Vec<WindowSpec>,
    lines_per_unit: usize,
    state: &mut ImportState,
    es_cfg: &EsConfig,
    import_opts: &Option<ImportOptions>,
) -> Result<(), error::Error> {
    let client = ElasticsearchClient::try_from(es_cfg)?;

    let alg_map = if let Some(algs) = &busco_config.algs {
        parse_alg_files(algs.clone())?
    } else {
        HashMap::new()
    };
    for busco_file in busco_config.files.as_ref().unwrap_or(&Vec::new()) {
        eprintln!("  Parsing BUSCO file: {}", busco_file.path.display());
        let full_table_reader =
            get_csv_reader(&Some(busco_file.path.clone()), b'\t', true, None, 2, true)?;
        // find all alg maps with matching lineage and apply them to the busco features
        let matching_algs: Vec<&AlgConfig> = alg_map
            .values()
            .filter(|alg| alg.lineage == busco_file.lineage)
            .collect();

        let mut busco_docs = Vec::new();
        let mut attribute_docs = Vec::new();
        let mut seen_attributes = std::collections::HashSet::new();

        for parsed_line in parse_full_table(full_table_reader) {
            match parsed_line {
                Ok(line) => {
                    if let Some(f) = line.feature {
                        let sequence_id = f.sequence.clone();
                        let sequence_feature = match sequence_features.get(&sequence_id) {
                            Some(sf) => sf,
                            None => {
                                eprintln!(
                                    "Warning: Sequence ID {} not found in sequence features. Skipping.",
                                    sequence_id
                                );
                                continue;
                            }
                        };
                        let sequence_length = sequence_feature.sequence_length;
                        let (start, end) = match f.start <= f.end {
                            true => (f.start, f.end),
                            false => (f.end, f.start),
                        };

                        // Tally counts for this sequence
                        state.busco_counts.add_to_sequence(
                            &sequence_id,
                            &busco_file.lineage,
                            &f.status.to_lowercase(),
                        );

                        let container_ids = overlapping_window_ids(
                            &sequence_id,
                            sequence_length,
                            start,
                            end,
                            &window_cfg,
                            lines_per_unit,
                        );

                        state.busco_id_tracker.record(
                            &f.id,
                            &sequence_id,
                            container_ids.clone(), // clone before moving
                            &f.status.to_lowercase(),
                            &busco_file.lineage,
                        );

                        // convert to FeatureDocument ready to index into Elasticsearch
                        let primary_type = format!("{}-busco-gene", busco_file.lineage);

                        let mut feature_document = FeatureDocument::new(
                            f.id.clone(),
                            Some(f.sequence.clone()),
                            primary_type,
                            start,
                            end,
                            Some(f.strand),
                            Some(container_ids),
                            sequence_id.clone(),
                            sequence_length,
                            busco_file.accession.clone(),
                            busco_file.taxon_id.clone(),
                            None,
                            Some(busco_file.path.to_string_lossy().to_string()),
                            Some("busco".to_string()),
                        );

                        // Initialize attributes if not present, but should always be present since we just created the FeatureDocument
                        if feature_document.attributes.is_none() {
                            feature_document.attributes = Some(Vec::new());
                        }

                        // add score and status as attributes
                        if let Some(attributes) = &mut feature_document.attributes {
                            attributes.push(NestedAttribute {
                                key: "busco_name".to_string(),
                                keyword_value: Some(super::genomehubs::StringOrVec::Single(
                                    f.id.clone(),
                                )),
                                ..Default::default()
                            });

                            attributes.push(NestedAttribute {
                                key: "busco_score".to_string(),
                                float_value: Some(f.score as f32),
                                ..Default::default()
                            });

                            attributes.push(NestedAttribute {
                                key: "busco_status".to_string(),
                                keyword_value: Some(super::genomehubs::StringOrVec::Single(
                                    f.status.to_lowercase(),
                                )),
                                ..Default::default()
                            });

                            attributes.push(NestedAttribute {
                                key: "assembly_id".to_string(),
                                keyword_value: Some(super::genomehubs::StringOrVec::Single(
                                    busco_config.accession.clone(),
                                )),
                                ..Default::default()
                            });

                            attributes.push(NestedAttribute {
                                key: "taxon_id".to_string(),
                                keyword_value: Some(super::genomehubs::StringOrVec::Single(
                                    busco_config.taxon_id.clone(),
                                )),
                                ..Default::default()
                            });

                            attributes.push(NestedAttribute {
                                key: "sequence_id".to_string(),
                                keyword_value: Some(super::genomehubs::StringOrVec::Single(
                                    sequence_id,
                                )),
                                ..Default::default()
                            });
                        }

                        // Apply ALG mappings
                        for alg in matching_algs.iter() {
                            if let Some(mapping) = &alg.mapping {
                                if let Some(mapped_id) = mapping.get(&f.id) {
                                    if let Some(attributes) = &mut feature_document.attributes {
                                        attributes.push(NestedAttribute {
                                            key: alg.name.clone(),
                                            keyword_value: Some(
                                                super::genomehubs::StringOrVec::Single(
                                                    mapped_id.clone(),
                                                ),
                                            ),
                                            ..Default::default()
                                        });
                                    }
                                }
                            }
                        }

                        // Create AttributeDocuments for each attribute type (once per file)
                        let attr_key_name = format!("busco_name");
                        if !seen_attributes.contains(&attr_key_name) {
                            attribute_docs.push(build_attribute_document(
                                &NestedAttribute {
                                    key: "busco_name".to_string(),
                                    keyword_value: Some(super::genomehubs::StringOrVec::Single(
                                        f.id.clone(),
                                    )),
                                    ..Default::default()
                                },
                                Some(&AttributeDocOverrides {
                                    display_name: Some("BUSCO Name".to_string()),
                                    description: Some("BUSCO locus name".to_string()),
                                    ..Default::default()
                                }),
                            ));
                            seen_attributes.insert(attr_key_name);
                        }

                        let attr_key_score = format!("busco_score");
                        if !seen_attributes.contains(&attr_key_score) {
                            attribute_docs.push(build_attribute_document(
                                &NestedAttribute {
                                    key: "busco_score".to_string(),
                                    float_value: Some(f.score as f32),
                                    ..Default::default()
                                },
                                Some(&AttributeDocOverrides {
                                    display_name: Some("BUSCO Score".to_string()),
                                    description: Some("BUSCO prediction score".to_string()),
                                    ..Default::default()
                                }),
                            ));
                            seen_attributes.insert(attr_key_score);
                        }

                        let attr_key_status = format!("busco_status");
                        if !seen_attributes.contains(&attr_key_status) {
                            attribute_docs.push(build_attribute_document(
                                &NestedAttribute {
                                    key: "busco_status".to_string(),
                                    keyword_value: Some(super::genomehubs::StringOrVec::Single(
                                        f.status.to_lowercase(),
                                    )),
                                    ..Default::default()
                                },
                                Some(&AttributeDocOverrides {
                                    display_name: Some("BUSCO Status".to_string()),
                                    description: Some("BUSCO gene prediction status".to_string()),
                                    ..Default::default()
                                }),
                            ));
                            seen_attributes.insert(attr_key_status);
                        }

                        // Create AttributeDocuments for ALG mappings
                        for alg in matching_algs.iter() {
                            let attr_key_alg = alg.name.clone();
                            if !seen_attributes.contains(&attr_key_alg) {
                                attribute_docs.push(build_attribute_document(
                                    &NestedAttribute {
                                        key: alg.name.clone(),
                                        keyword_value: Some(
                                            super::genomehubs::StringOrVec::Single(
                                                alg.name.clone(),
                                            ),
                                        ),
                                        ..Default::default()
                                    },
                                    Some(&AttributeDocOverrides {
                                        display_name: Some(format!("BUSCO {}", alg.name)),
                                        description: Some(format!("ALG mapping for {}", alg.name)),
                                        ..Default::default()
                                    }),
                                ));
                                seen_attributes.insert(attr_key_alg);
                            }
                        }

                        busco_docs.push(feature_document.clone());
                        // feature_map.insert(f.id.clone(), feature_document);
                    } else {
                        // Incomplete line - BUSCO is missing from assembly
                        state.busco_counts.add_missing();
                        state.busco_id_tracker.record(
                            &line.id,
                            "MISSING",
                            vec![],
                            "missing",
                            &busco_file.lineage,
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Error parsing BUSCO line: {}", e);
                }
            }
        }

        // Index BUSCO feature documents after each file completes
        if !busco_docs.is_empty() {
            eprintln!("    Indexing {} BUSCO features", busco_docs.len());
            let wrapped_docs = client.wrap_for_bulk_index(busco_docs)?;
            client.index_documents("feature", wrapped_docs)?;
            client.refresh("feature")?;
        }

        if !attribute_docs.is_empty() {
            sync_attribute_documents(attribute_docs, state, es_cfg, import_opts)?;
        }
    }
    Ok(())
}
