//! Functions to parse a busco full table file and return an iterator of BuscoFeature structs.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;

use anyhow;
use serde::{Deserialize, Serialize};

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

fn parse_full_table(
    mut full_table_reader: csv::Reader<Box<dyn BufRead>>,
) -> impl Iterator<Item = Result<BuscoFeature, anyhow::Error>> {
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
            Ok(BuscoFeature {
                id,
                status,
                score,
                sequence,
                start,
                end,
                strand,
                length,
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
            b' ',
            false,
            None,
            0,
            true,
        )?;
        let mut mapping = HashMap::new();
        for result in alg_reader.into_records() {
            let record = result?;
            dbg!(&record);
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

pub fn parse_busco_files(
    busco_config: &MultiBuscoConfig,
    sequence_features: &HashMap<String, FeatureDocument>,
    window_cfg: Vec<WindowSpec>,
) -> Result<HashMap<String, FeatureDocument>, anyhow::Error> {
    let mut feature_map = HashMap::new();
    let alg_map = if let Some(algs) = &busco_config.algs {
        parse_alg_files(algs.clone())?
    } else {
        HashMap::new()
    };
    for busco_file in busco_config.files.as_ref().unwrap_or(&Vec::new()) {
        let full_table_reader =
            get_csv_reader(&Some(busco_file.path.clone()), b'\t', true, None, 2, true)?;
        // find all alg maps with matching lineage and apply them to the busco features
        let matching_algs: Vec<&AlgConfig> = alg_map
            .values()
            .filter(|alg| alg.lineage == busco_file.lineage)
            .collect();
        for feature in parse_full_table(full_table_reader) {
            match feature {
                Ok(f) => {
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
                    let primary_type = format!("{}-busco-gene", busco_file.lineage);
                    let container_ids = overlapping_window_ids(
                        &sequence_id,
                        sequence_length,
                        start,
                        end,
                        &window_cfg,
                        1000, // or pass from config
                    );
                    let container_ids = Some(container_ids);
                    // convert to FeatureDocument ready to index into Elasticsearch
                    let mut feature_document = FeatureDocument::new(
                        f.id.clone(),
                        Some(f.sequence.clone()),
                        primary_type,
                        start,
                        end,
                        Some(f.strand),
                        container_ids,
                        sequence_id,
                        sequence_length,
                        busco_file.accession.clone(),
                        busco_file.taxon_id.clone(),
                        None,
                        Some(busco_file.path.to_string_lossy().to_string()),
                        Some("busco".to_string()),
                    );
                    // add score and status as attributes
                    if let Some(attributes) = &mut feature_document.attributes {
                        attributes.push(NestedAttribute {
                            key: "score".to_string(),
                            float_value: Some(f.score as f32),
                            ..Default::default()
                        });

                        attributes.push(NestedAttribute {
                            key: "status".to_string(),
                            keyword_value: Some(super::genomehubs::StringOrVec::Single(
                                f.status.clone(),
                            )),
                            ..Default::default()
                        });
                    }
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
                    feature_map.insert(f.id.clone(), feature_document);
                }
                Err(e) => {
                    // Handle the error
                    eprintln!("Error parsing feature: {}", e);
                }
            }
        }
    }
    Ok(feature_map)
}
