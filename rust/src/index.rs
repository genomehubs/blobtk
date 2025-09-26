//!
//! Invoked by calling:
//! `blobtk index <args>`

use std::collections::HashMap;
use std::collections::HashSet;
use std::io::BufRead;
use std::path::PathBuf;

use anyhow;
use log;
use numfmt::Formatter;
use numfmt::Precision;
use schemars::schema_for;
use serde::de::{self, Deserializer};
use serde::Deserialize;
use serde_json::to_string_pretty;

use crate::blobdir;
use crate::cli;
use crate::io::file_reader;
use crate::io::get_csv_reader;
use crate::io::get_file_writer;
use crate::io::get_writer;
use crate::parse::genomehubs::ConstraintConfig;
use crate::parse::genomehubs::FieldType;
use crate::parse::genomehubs::GHubsAnalysisConfig;
use crate::parse::genomehubs::GHubsConfig;
use crate::parse::genomehubs::GHubsFieldConfig;
use crate::parse::genomehubs::GHubsFileConfig;
use crate::parse::genomehubs::GHubsFileFormat;
use crate::parse::genomehubs::PathBufOrVec;
use crate::parse::genomehubs::StringOrVec;
use crate::parse::genomehubs::SummaryFunction;
use crate::parse::genomehubs::SummaryFunctionOrVec;
use crate::parse::genomehubs::TraverseDirection;
use std::process::Command;

pub use cli::IndexOptions;

#[derive(Debug)]
pub struct Analysis {
    pub analysis_id: String,
    pub assembly_id: String,
    pub analysis_type: String,
    pub name: String,
    pub description: String,
    pub taxon_id: String,
    pub title: String,
    pub date: String,
    pub version: String,
}

#[derive(Debug, Default)]
pub struct Feature {
    pub feature_id: String,
    pub sequence_id: String,
    pub feature_type: String,
    pub start: usize,
    pub end: usize,
    pub strand: i8,
    pub length: usize,
    pub gc: Option<f64>,
    pub coverage: Option<f64>,
    pub masked: Option<f64>,
    pub midpoint: usize,
    pub midpoint_proportion: f64,
    pub seq_proportion: f64,
    pub name: Option<String>,
    pub sequence_name: Option<String>,
    pub score: Option<f64>,
    pub status: Option<String>,
    pub busco_counts: Option<HashMap<String, usize>>,
}

impl Feature {
    pub fn new(
        feature_id: String,
        sequence_id: String,
        feature_type: String,
        start: usize,
        end: usize,
        strand: i8,
        length: usize,
        gc: Option<f64>,
        coverage: Option<f64>,
        masked: Option<f64>,
        midpoint: usize,
        midpoint_proportion: f64,
        seq_proportion: f64,
        name: Option<String>,
        sequence_name: Option<String>,
        score: Option<f64>,
        status: Option<String>,
        busco_counts: Option<HashMap<String, usize>>,
    ) -> Self {
        Self {
            feature_id,
            sequence_id,
            feature_type,
            start,
            end,
            strand,
            length,
            gc,
            coverage,
            masked,
            midpoint,
            midpoint_proportion,
            seq_proportion,
            name,
            sequence_name,
            score,
            status,
            busco_counts,
        }
    }

    pub fn to_string(
        &self,
        taxon_id: &String,
        assembly_id: &String,
        busco_count: Option<usize>,
    ) -> String {
        let busco_counts_str = if let Some(busco_counts) = &self.busco_counts {
            busco_counts
                .iter()
                .map(|(_, value)| format!("{}", value))
                .collect::<Vec<String>>()
                .join("\t")
        } else if let Some(busco_count) = busco_count {
            (0..busco_count)
                .map(|_| "None".to_string())
                .collect::<Vec<String>>()
                .join("\t")
        } else {
            "None".to_string()
        };

        let mut f = Formatter::new();
        f = f.precision(Precision::Significance(4));

        let gc_str = self
            .gc
            .map_or("None".to_string(), |v| f.fmt2(v).to_string());
        let coverage_str = self
            .coverage
            .map_or("None".to_string(), |v| f.fmt2(v).to_string());
        let masked_str = self
            .masked
            .map_or("None".to_string(), |v| f.fmt2(v).to_string());
        let score_str = self
            .score
            .map_or("None".to_string(), |v| f.fmt2(v).to_string());
        let midpoint_proportion_str = f.fmt2(self.midpoint_proportion).to_string();
        let seq_proportion_str = f.fmt2(self.seq_proportion).to_string();

        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.feature_id,
            taxon_id,
            assembly_id,
            self.sequence_id,
            self.feature_type,
            self.start,
            self.end,
            self.strand,
            self.length,
            gc_str,
            coverage_str,
            masked_str,
            self.midpoint,
            midpoint_proportion_str,
            seq_proportion_str,
            self.name.as_ref().unwrap_or(&"None".to_string()),
            self.sequence_name.as_ref().unwrap_or(&"None".to_string()),
            score_str,
            self.status.as_ref().unwrap_or(&"None".to_string()),
            busco_counts_str
        )
    }

    pub fn to_header(&self) -> String {
        let mut header = "feature_id\ttaxon_id\tassembly_id\tsequence_id\tfeature_type\tstart\tend\tstrand\tlength\tgc\tcoverage\tmasked\tmidpoint\tmidpoint_proportion\tseq_proportion\tname\tsequence_name\tscore\tstatus".to_string();
        if let Some(ref busco_counts) = self.busco_counts {
            let keys = busco_counts.keys();
            for key in keys {
                header.push_str(&format!("\t{}", key));
            }
        } else {
            header.push_str("\tbusco_counts");
        }
        header
    }
}

#[derive(Debug, Default)]
pub struct Features {
    pub taxon_id: String,
    pub assembly_id: String,
    pub window_size: f64,
    pub busco_count: Option<usize>,
    pub features: Vec<Feature>,
}

impl Features {
    pub fn new(
        taxon_id: String,
        assembly_id: String,
        window_size: f64,
        features: Vec<Feature>,
        busco_count: Option<usize>,
    ) -> Self {
        Self {
            taxon_id,
            assembly_id,
            window_size,
            features,
            busco_count,
        }
    }

    pub fn from_vecs(
        taxon_id: String,
        assembly_id: String,
        feature_type: String,
        ids: Vec<String>,
        lengths: Vec<usize>,
        strands: Option<Vec<i8>>,
        gcs: Option<Vec<f64>>,
        coverages: Option<Vec<f64>>,
        maskeds: Option<Vec<f64>>,
        names: Option<Vec<String>>,
        sequence_names: Option<Vec<String>>,
        scores: Option<Vec<f64>>,
        statuses: Option<Vec<String>>,
        busco_counts: Option<HashMap<String, Vec<usize>>>,
    ) -> Self {
        let mut features = Vec::new();
        let span = lengths.iter().sum::<usize>();
        for (i, id) in ids.iter().enumerate() {
            let feature_id = format!("{}:{}", id, feature_type);
            let sequence_id = id.clone();
            let feature_type = feature_type.clone();
            let start = 1;
            let end = lengths[i];
            let strand = if let Some(strands) = &strands {
                strands[i]
            } else {
                1
            };
            let length = lengths[i];
            let gc = if let Some(gcs) = &gcs {
                Some(gcs[i])
            } else {
                None
            };
            let coverage = if let Some(coverages) = &coverages {
                Some(coverages[i])
            } else {
                None
            };
            let masked = if let Some(maskeds) = &maskeds {
                Some(maskeds[i])
            } else {
                None
            };
            let name = if let Some(names) = &names {
                Some(names[i].clone())
            } else {
                None
            };
            let sequence_name = if let Some(sequence_names) = &sequence_names {
                Some(sequence_names[i].clone())
            } else {
                None
            };
            let score = if let Some(scores) = &scores {
                Some(scores[i])
            } else {
                None
            };
            let status = if let Some(statuses) = &statuses {
                Some(statuses[i].clone())
            } else {
                None
            };
            let feature_busco_counts = if let Some(all_busco_counts) = &busco_counts {
                // make a hashmap of busco counts for this feature
                let mut _busco_counts = HashMap::new();
                for (busco, counts) in all_busco_counts {
                    _busco_counts.insert(busco.clone(), counts[i]);
                }
                Some(_busco_counts)
            } else {
                None
            };
            let midpoint = (start + end) / 2;
            let midpoint_proportion = midpoint as f64 / length as f64;
            let seq_proportion = length as f64 / span as f64;
            features.push(Feature {
                feature_id,
                sequence_id,
                feature_type,
                start,
                end,
                strand,
                length,
                gc,
                coverage,
                masked,
                midpoint,
                midpoint_proportion,
                seq_proportion,
                name,
                sequence_name,
                score,
                status,
                busco_counts: feature_busco_counts,
            });
        }
        let busco_count = if let Some(busco_counts) = &busco_counts {
            Some(busco_counts.len())
        } else {
            None
        };
        Self::new(taxon_id, assembly_id, 1.0, features, busco_count)
    }

    pub fn from_vec_of_vecs(
        taxon_id: String,
        assembly_id: String,
        window_size: f64,
        feature_type: String,
        ids: Vec<String>,
        lengths: Vec<Vec<usize>>,
        strands: Option<Vec<Vec<i8>>>,
        gcs: Option<Vec<Vec<f64>>>,
        coverages: Option<Vec<Vec<f64>>>,
        maskeds: Option<Vec<Vec<f64>>>,
        names: Option<Vec<Vec<String>>>,
        sequence_names: Option<Vec<Vec<String>>>,
        scores: Option<Vec<Vec<f64>>>,
        statuses: Option<Vec<Vec<String>>>,
        busco_counts: Option<HashMap<String, Vec<Vec<usize>>>>,
        sequences: &HashMap<String, &Feature>,
    ) -> Self {
        let mut features = Vec::new();
        for (i, id) in ids.iter().enumerate() {
            let mut start = 1;
            let span = lengths[i].iter().sum::<usize>();
            for (j, length) in lengths[i].iter().enumerate() {
                let length = length.clone();
                let end = start + length - 1;
                let feature_id = format!(
                    "{}:{}-{}:{}",
                    id,
                    start,
                    end,
                    feature_type.split(',').next().unwrap_or(&feature_type)
                );
                let sequence_id = id.clone();
                let seq_feature = sequences.get(id).unwrap();
                let feature_type = feature_type.clone();
                let strand = if let Some(strands) = &strands {
                    strands[i][j]
                } else {
                    1
                };
                let gc = if let Some(gcs) = &gcs {
                    Some(gcs[i][j])
                } else {
                    None
                };
                let coverage = if let Some(coverages) = &coverages {
                    Some(coverages[i][j])
                } else {
                    None
                };
                let masked = if let Some(maskeds) = &maskeds {
                    Some(maskeds[i][j])
                } else {
                    None
                };
                let name = if let Some(names) = &names {
                    Some(names[i][j].clone())
                } else {
                    None
                };
                let sequence_name = if let Some(sequence_names) = &sequence_names {
                    Some(sequence_names[i][j].clone())
                } else {
                    None
                };
                let score = if let Some(scores) = &scores {
                    Some(scores[i][j])
                } else {
                    None
                };
                let status = if let Some(statuses) = &statuses {
                    Some(statuses[i][j].clone())
                } else {
                    None
                };
                let feature_busco_counts = if let Some(all_busco_counts) = &busco_counts {
                    // make a hashmap of busco counts for this feature
                    let mut _busco_counts = HashMap::new();
                    for (busco, counts) in all_busco_counts {
                        _busco_counts.insert(busco.clone(), counts[i][j]);
                    }
                    Some(_busco_counts)
                } else {
                    None
                };
                let midpoint = (start + end) / 2;
                let midpoint_proportion = midpoint as f64 / seq_feature.length as f64;
                let seq_proportion = length as f64 / span as f64;
                features.push(Feature {
                    feature_id,
                    sequence_id,
                    feature_type,
                    start,
                    end,
                    strand,
                    length,
                    gc,
                    coverage,
                    masked,
                    midpoint,
                    midpoint_proportion,
                    seq_proportion,
                    name,
                    sequence_name,
                    score,
                    status,
                    busco_counts: feature_busco_counts,
                });
                start += length;
            }
        }
        let busco_count = if let Some(busco_counts) = &busco_counts {
            Some(busco_counts.len())
        } else {
            None
        };
        Self::new(taxon_id, assembly_id, window_size, features, busco_count)
    }

    pub fn to_string(&self) -> String {
        let mut output = Vec::new();
        for feature in &self.features {
            output.push(feature.to_string(&self.taxon_id, &self.assembly_id, self.busco_count));
        }
        output.join("\n")
    }

    pub fn to_header(&self) -> String {
        Feature::to_header(&self.features[0])
    }

    pub fn to_tsv(&self) -> String {
        let mut output = Vec::new();
        output.push(self.to_header());
        output.push(self.to_string());
        output.join("\n")
    }

    pub fn to_file(&self, file_path: &Option<PathBuf>) -> Result<(), anyhow::Error> {
        let mut writer = get_writer(file_path);
        writeln!(&mut writer, "{}", self.to_tsv())?;
        Ok(())
    }

    pub fn append_to_file(&self, file_path: &Option<PathBuf>) -> Result<(), anyhow::Error> {
        if let Some(file_path) = file_path {
            let mut writer = get_file_writer(file_path, true);
            writeln!(&mut writer, "{}", self.to_string())?;
        }
        Ok(())
    }

    pub fn to_ghubs_config(
        &self,
        file: Option<PathBuf>,
        analysis: Option<GHubsAnalysisConfig>,
    ) -> GHubsConfig {
        let mut attributes = HashMap::new();
        let fields = vec![
            ("feature_id", FieldType::Keyword, None),
            ("feature_type", FieldType::Keyword, Some(",")),
            ("name", FieldType::Keyword, Some(",")),
            ("sequence_id", FieldType::Keyword, None),
            ("sequence_name", FieldType::Keyword, Some(",")),
            // ("analysis_name", FieldType::Keyword, None),
            ("taxon_id", FieldType::Keyword, None),
            ("assembly_id", FieldType::Keyword, None),
            ("start", FieldType::Long, None),
            ("end", FieldType::Long, None),
            ("strand", FieldType::Byte, None),
            ("length", FieldType::Long, None),
            ("gc", FieldType::ThreeDP, None),
            ("coverage", FieldType::TwoDP, None),
            ("masked", FieldType::ThreeDP, None),
            ("midpoint", FieldType::Long, None),
            ("midpoint_proportion", FieldType::Float, None),
            ("seq_proportion", FieldType::Float, None),
            ("score", FieldType::HalfFloat, None),
            ("status", FieldType::Keyword, Some(",")),
        ];
        for (field, field_type, separator) in fields {
            attributes.insert(
                field.to_string(),
                GHubsFieldConfig {
                    header: Some(StringOrVec::Single(field.to_string())),
                    separator: match separator {
                        Some(s) => Some(StringOrVec::Single(s.to_string())),
                        None => None,
                    },
                    field_type,
                    ..Default::default()
                },
            );
        }
        let mut taxonomy = HashMap::new();
        taxonomy.insert(
            "taxon_id".to_string(),
            GHubsFieldConfig {
                header: Some(StringOrVec::Single("taxon_id".to_string())),
                ..Default::default()
            },
        );
        let mut features = HashMap::new();
        features.insert(
            "feature_id".to_string(),
            GHubsFieldConfig {
                header: Some(StringOrVec::Single("feature_id".to_string())),
                ..Default::default()
            },
        );
        features.insert(
            "assembly_id".to_string(),
            GHubsFieldConfig {
                header: Some(StringOrVec::Single("assembly_id".to_string())),
                ..Default::default()
            },
        );
        let config = GHubsConfig {
            file: match file {
                Some(f) => Some(GHubsFileConfig {
                    format: GHubsFileFormat::TSV,
                    header: true,
                    name: PathBuf::from(f.file_name().unwrap()),
                    needs: Some(PathBufOrVec::Single(PathBuf::from(
                        "ATTR_feature.types.yaml".to_string(),
                    ))),
                    ..Default::default()
                }),
                None => None,
            },
            attributes: Some(attributes),
            taxonomy: Some(taxonomy),
            features: Some(features),
            analysis,
            ..Default::default()
        };

        config
    }
}

fn per_contig_values(
    meta: &blobdir::Meta,
    blobdir_path: &PathBuf,
) -> Result<Features, anyhow::Error> {
    let taxon_id = meta.taxon.taxid.clone();
    let assembly_id = meta.assembly.accession.clone();
    let plot_meta = meta.plot.clone();
    let identifiers = blobdir::parse_field_identifiers("identifiers".to_string(), &blobdir_path)?;
    let gc_values = blobdir::parse_field_float("gc".to_string(), &blobdir_path)?;
    let length_values = blobdir::parse_field_int("length".to_string(), &blobdir_path)?;
    let coverage_values = if let Some(coverage) = plot_meta.y {
        blobdir::parse_field_float(coverage, &blobdir_path)?
    } else {
        vec![0.0; length_values.len()]
    };
    let masked_values = match blobdir::parse_field_float("masked".to_string(), &blobdir_path) {
        Ok(masked_values) => Some(masked_values),
        Err(_) => None,
    };
    let busco_counts = if let Some(busco_list) = &meta.busco_list {
        let mut _busco_counts = HashMap::new();
        for busco in busco_list {
            let field_id = format!("{}_count", busco.2);
            if let Ok(busco_values) = blobdir::parse_field_int(field_id.clone(), &blobdir_path) {
                _busco_counts.insert(field_id.clone(), busco_values);
            }
        }
        Some(_busco_counts)
    } else {
        None
    };
    let features = Features::from_vecs(
        taxon_id,
        assembly_id,
        "topLevel".to_string(),
        identifiers,
        length_values,
        None,
        Some(gc_values),
        Some(coverage_values),
        masked_values,
        None,
        None,
        None,
        None,
        busco_counts,
    );
    Ok(features)
}

fn get_window_id(id: &str, window_size: &f64) -> String {
    if window_size == &1.0 {
        format!("{}", id)
    } else if window_size == &0.1 {
        format!("{}_windows", id)
    } else {
        format!("{}_windows_{}", id, window_size)
    }
}

fn per_window_values(
    meta: &blobdir::Meta,
    blobdir_path: &PathBuf,
    sequences: &HashMap<String, &Feature>,
    window_size: &f64,
) -> Result<Features, anyhow::Error> {
    let plot_meta = meta.plot.clone();
    let taxon_id = meta.taxon.taxid.clone();
    let assembly_id = meta.assembly.accession.clone();

    let identifiers = blobdir::parse_field_identifiers("identifiers".to_string(), &blobdir_path)?;
    let gc_values =
        blobdir::parse_field_float_windows(get_window_id("gc", window_size), &blobdir_path, None)?;
    let length_values = blobdir::parse_field_int_windows(
        get_window_id("length", window_size),
        &blobdir_path,
        None,
    )?;
    let coverage_values = if let Some(coverage) = plot_meta.y {
        Some(
            blobdir::parse_field_float_windows(
                get_window_id(&coverage, window_size),
                &blobdir_path,
                None,
            )?
            .0,
        )
    } else {
        None
    };
    let masked_values = match blobdir::parse_field_float_windows(
        get_window_id("masked", window_size),
        &blobdir_path,
        None,
    ) {
        Ok(masked_values) => Some(masked_values.0),
        Err(_) => None,
    };
    let busco_counts = if let Some(busco_list) = &meta.busco_list {
        let mut _busco_counts = HashMap::new();
        for busco in busco_list {
            let field_name = format!("{}_count", busco.2);
            let field_id = get_window_id(&field_name, window_size);
            let busco_values =
                match blobdir::parse_field_int_windows(field_id.clone(), &blobdir_path, None) {
                    Ok(values) => values.0,
                    Err(_) => continue,
                };
            _busco_counts.insert(field_name, busco_values);
        }
        Some(_busco_counts)
    } else {
        None
    };
    let features = Features::from_vec_of_vecs(
        taxon_id,
        assembly_id,
        *window_size,
        format!("window-{},window", window_size),
        identifiers,
        length_values.0,
        None,
        Some(gc_values.0),
        coverage_values,
        masked_values,
        None,
        None,
        None,
        None,
        busco_counts,
        sequences,
    );
    Ok(features)
}

fn parse_full_table(
    mut full_table_reader: csv::Reader<Box<dyn BufRead>>,
) -> impl Iterator<Item = Result<(String, String, f64, String, usize, usize, i8, usize), anyhow::Error>>
{
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
            Ok((id, status, score, sequence, start, end, strand, length))
        } else {
            Err(anyhow::anyhow!("Error reading record"))
        }
    })
}

fn _busco_analysis(meta: &blobdir::Meta, busco: &(String, usize, String)) -> Analysis {
    let assembly_id = meta.assembly.accession.clone();
    let lineage = busco.2.clone();
    let busco_version = "5"; // hard-coded for now
    let analysis_id = format!("busco{}-{}_{}", busco_version, lineage, assembly_id);
    let analysis_type = "busco".to_string();
    let name = format!("BUSCO_{}", lineage);
    let description = format!(
        "BUSCO v{} analysis of {} using lineage {}",
        busco_version, assembly_id, lineage
    );
    let taxon_id = meta.taxon.taxid.clone();
    let title = format!("BUSCO v{} {} {}", busco_version, assembly_id, lineage);
    let date = "1970-01-01".to_string(); // set to 1970-01-01 for now
    let version = busco_version.to_string(); //set to busco version for now
    Analysis {
        analysis_id,
        assembly_id,
        analysis_type,
        name,
        description,
        taxon_id,
        title,
        date,
        version,
    }
}

fn window_analysis(meta: &blobdir::Meta, window_size: &f64) -> Analysis {
    let assembly_id = meta.assembly.accession.clone();
    let analysis_id = format!("window-{}", window_size);
    let analysis_type = "window".to_string();
    let name = format!("window-{}", window_size);
    let description = format!(
        "Window analysis of {} using window size {}",
        assembly_id, window_size
    );
    let taxon_id = meta.taxon.taxid.clone();
    let title = format!(
        "Window analysis of {} using window size {}",
        assembly_id, window_size
    );
    let date = "1970-01-01".to_string(); // set to 1970-01-01 for now
    let version = "1".to_string(); //set to 1 for now
    Analysis {
        analysis_id,
        assembly_id,
        analysis_type,
        name,
        description,
        taxon_id,
        title,
        date,
        version,
    }
}

// {
//     "parameters": {
//         "out_path": "/tmp/nxf.KJrEhYgsfm",
//         "cpu": "16",
//         "force": "True",
//         "restart": "False",
//         "quiet": "False",
//         "download_path": "/tmp/nxf.KJrEhYgsfm/v5",
//         "datasets_version": "odb10",
//         "offline": "True",
//         "download_base_url": "https://busco-data.ezlab.org/v5/data/",
//         "auto-lineage": "False",
//         "auto-lineage-prok": "False",
//         "auto-lineage-euk": "False",
//         "update-data": "False",
//         "use_augustus": "False",
//         "batch_mode": "True",
//         "tar": "True",
//         "in": "/tmp/nxf.KJrEhYgsfm/input_seqs/GCA_025594145.1_ASM2559414v1_genomic.fna",
//         "out": "GCA_025594145.1_ASM2559414v1_genomic-eukaryota_odb10-busco",
//         "mode": "euk_genome_met",
//         "lineage_dataset": "/tmp/nxf.KJrEhYgsfm/v5/lineages/eukaryota_odb10",
//         "main_out": "/tmp/nxf.KJrEhYgsfm/GCA_025594145.1_ASM2559414v1_genomic-eukaryota_odb10-busco/GCA_025594145.1_ASM2559414v1_genomic.fna",
//         "lineage_results_dir": "run_eukaryota_odb10",
//         "domain": "eukaryota",
//         "max_intron": "130000",
//         "max_seq_len": "160000",
//         "metaeuk_parameters": "",
//         "metaeuk_rerun_parameters": "",
//         "contig_break": "10",
//         "scaffold_composition": "False",
//         "gene_predictor": "metaeuk"
//     },
//     "lineage_dataset": {
//         "name": "eukaryota_odb10",
//         "creation_date": "2020-09-10",
//         "number_of_buscos": "255",
//         "number_of_species": "70"
//     },
//     "versions": {
//         "hmmsearch": 3.1,
//         "bbtools": "38.98",
//         "metaeuk": "6.a5d39d9",
//         "busco": "5.4.3"
//     },
//     "results": {
//         "one_line_summary": "C:97.6%[S:59.6%,D:38.0%],F:1.6%,M:0.8%,n:255",
//         "Complete": 97.6,
//         "Single copy": 59.6,
//         "Multi copy": 38.0,
//         "Fragmented": 1.6,
//         "Missing": 0.8,
//         "n_markers": 255,
//         "domain": "eukaryota",
//         "Number of scaffolds": "669",
//         "Number of contigs": "2667",
//         "Total length": "804232818",
//         "Percent gaps": "0.136%",
//         "Scaffold N50": "41970859",
//         "Contigs N50": "3476787"
//     }
// }

#[derive(Debug, Deserialize)]
struct BuscoResults {
    #[serde(alias = "Complete", rename = "Complete percentage")]
    complete: f64,
    #[serde(alias = "Single copy", rename = "Single copy percentage")]
    single_copy: f64,
    #[serde(alias = "Multi copy", rename = "Multi copy percentage")]
    multi_copy: f64,
    #[serde(alias = "Fragmented", rename = "Fragmented percentage")]
    fragmented: f64,
    #[serde(alias = "Missing", rename = "Missing percentage")]
    missing: f64,
    n_markers: usize,
    domain: String,
    #[serde(alias = "Number of scaffolds", deserialize_with = "string_or_usize")]
    number_of_scaffolds: usize,
    #[serde(alias = "Number of contigs", deserialize_with = "string_or_usize")]
    number_of_contigs: usize,
    #[serde(alias = "Total length", deserialize_with = "string_or_usize")]
    total_length: usize,
    #[serde(alias = "Percent gaps", deserialize_with = "percent_string_or_f64")]
    percent_gaps: f64,
    #[serde(alias = "Scaffold N50", deserialize_with = "string_or_usize")]
    scaffold_n50: usize,
    #[serde(alias = "Contigs N50", deserialize_with = "string_or_usize")]
    contigs_n50: usize,
}

fn string_or_usize<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: serde_json::Value = serde::Deserialize::deserialize(deserializer)?;
    match value {
        serde_json::Value::Number(num) => num
            .as_u64()
            .map(|n| n as usize)
            .ok_or_else(|| serde::de::Error::custom("Invalid number")),
        serde_json::Value::String(s) => s
            .parse::<usize>()
            .map_err(|_| serde::de::Error::custom("Invalid string for usize")),
        _ => Err(serde::de::Error::custom("Expected a number or string")),
    }
}

fn percent_string_or_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let value: serde_json::Value = serde::Deserialize::deserialize(deserializer)?;
    match value {
        serde_json::Value::Number(num) => num
            .as_f64()
            .ok_or_else(|| de::Error::custom("Invalid number for f64")),
        serde_json::Value::String(s) => {
            let trimmed = s.trim_end_matches('%');
            trimmed
                .parse::<f64>()
                .map_err(|_| de::Error::custom("Invalid string for f64"))
        }
        _ => Err(de::Error::custom(
            "Expected a number or a percentage string",
        )),
    }
}

#[derive(Debug, Deserialize)]
struct BuscoLineageDataset {
    name: String,
    creation_date: String,
    #[serde(deserialize_with = "string_or_usize")]
    number_of_buscos: usize,
    #[serde(deserialize_with = "string_or_usize")]
    number_of_species: usize,
}
#[derive(Debug, Deserialize)]
struct BuscoVersions {
    #[serde(deserialize_with = "numeric_or_string_to_string")]
    hmmsearch: String,
    #[serde(deserialize_with = "numeric_or_string_to_string")]
    bbtools: String,
    #[serde(deserialize_with = "numeric_or_string_to_string")]
    metaeuk: String,
    #[serde(deserialize_with = "numeric_or_string_to_string")]
    busco: String,
}

fn numeric_or_string_to_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value: serde_json::Value = serde::Deserialize::deserialize(deserializer)?;
    match value {
        serde_json::Value::Number(num) => Ok(num.to_string()),
        serde_json::Value::String(s) => Ok(s),
        _ => Err(de::Error::custom("Expected a number or string")),
    }
}

#[derive(Debug, Deserialize)]
struct BuscoSummary {
    results: BuscoResults,
    lineage_dataset: BuscoLineageDataset,
    versions: BuscoVersions,
}

#[derive(Debug, Default)]
struct BuscoCounts {
    complete: usize,
    fragmented: usize,
    missing: usize,
    duplicated: usize,
    single: usize,
}

impl BuscoCounts {
    fn to_field_config(lineage: String, status: String) -> GHubsFieldConfig {
        let status = status.to_lowercase();
        let summary_function = match status.as_str() {
            "single" => SummaryFunction::Max,
            "complete" => SummaryFunction::Max,
            _ => SummaryFunction::Min,
        };
        let lineage = lineage.to_lowercase();
        let header = format!("{}_{}_count", lineage, status);
        let field_config = GHubsFieldConfig {
            header: Some(StringOrVec::Single(header.clone())),
            description: Some(format!("Count of {} {} genes", lineage, status)),
            name: Some(header.clone()),
            field_type: FieldType::Short,
            taxon_display_level: Some(2),
            taxon_summary: Some(SummaryFunctionOrVec::Single(summary_function.clone())),
            taxon_traverse: Some(summary_function),
            taxon_traverse_direction: Some(TraverseDirection::Up),
            taxon_traverse_limit: Some(lineage.split('_').next().unwrap_or("").to_string()),
            ..Default::default()
        };
        field_config
    }
}

#[derive(Debug, Default)]
struct BuscoProportions {
    complete: f64,
    fragmented: f64,
    missing: f64,
    duplicated: f64,
    single: f64,
}

impl BuscoProportions {
    fn to_field_config(lineage: String, status: String) -> GHubsFieldConfig {
        let status = status.to_lowercase();
        let summary_function = match status.as_str() {
            "single" => SummaryFunction::Max,
            "complete" => SummaryFunction::Max,
            _ => SummaryFunction::Min,
        };
        let lineage = lineage.to_lowercase();
        let header = format!("{}_{}_proportion", lineage, status);
        let field_config = GHubsFieldConfig {
            header: Some(StringOrVec::Single(header.clone())),
            description: Some(format!("Proportion of {} {} genes", lineage, status)),
            name: Some(header.clone()),
            field_type: FieldType::TwoDP,
            taxon_display_level: Some(2),
            taxon_summary: Some(SummaryFunctionOrVec::Single(summary_function.clone())),
            taxon_traverse: Some(summary_function),
            taxon_traverse_direction: Some(TraverseDirection::Up),
            taxon_traverse_limit: Some(lineage.split('_').next().unwrap_or("").to_string()),
            ..Default::default()
        };
        field_config
    }
}

#[derive(Debug, Default)]
struct BuscoLists {
    complete: Vec<String>,
    fragmented: Vec<String>,
    missing: Vec<String>,
    duplicated: Vec<String>,
    single: Vec<String>,
}

impl BuscoLists {
    fn to_field_config(lineage: String, status: String) -> GHubsFieldConfig {
        let status = status.to_lowercase();
        let order = match status.as_str() {
            "duplicated" => vec![
                format!("{}_duplicated", lineage),
                format!("{}_single", lineage),
            ],
            "single" => vec![
                format!("{}_duplicated", lineage),
                format!("{}_single", lineage),
            ],
            _ => vec![
                format!("{}_complete", lineage),
                format!("{}_fragmented", lineage),
                format!("{}_missing", lineage),
            ],
        };
        let lineage = lineage.to_lowercase();
        let header = format!("{}_{}", lineage, status);

        let field_config = GHubsFieldConfig {
            header: Some(StringOrVec::Single(header.clone())),
            description: Some(format!("List of {} {} genes", lineage, status)),
            display_group: Some("busco".to_string()),
            display_level: Some(2),
            name: Some(header.clone()),
            list_key: Some(header),
            order: Some(order),
            return_type: Some(SummaryFunction::Length),
            separator: Some(StringOrVec::Single(",".to_string())),
            summary: Some(SummaryFunctionOrVec::Single(SummaryFunction::List)),
            taxon_display_level: Some(2),
            taxon_summary: Some(SummaryFunctionOrVec::Single(SummaryFunction::OrderedList)),
            taxon_traverse: Some(SummaryFunction::OrderedList),
            taxon_traverse_direction: Some(TraverseDirection::Up),
            taxon_traverse_limit: Some(lineage.split('_').next().unwrap_or("").to_string()),
            field_type: FieldType::Keyword,
            ..Default::default()
        };
        field_config
    }
}

#[derive(Debug, Default)]
struct BuscoStats {
    taxon_id: String,
    assembly_id: String,
    lineage: String,
    count: usize,
    proportions: BuscoProportions,
    lists: BuscoLists,
    counts: BuscoCounts,
}

impl BuscoStats {
    pub fn new(
        taxon_id: String,
        assembly_id: String,
        lineage: String,
        count: usize,
        proportions: BuscoProportions,
        lists: BuscoLists,
        counts: BuscoCounts,
    ) -> Self {
        Self {
            taxon_id,
            assembly_id,
            lineage,
            count,
            proportions,
            lists,
            counts,
        }
    }

    pub fn from_summary(
        taxon_id: String,
        assembly_id: String,
        lineage: String,
        summary: BuscoSummary,
        by_status: HashMap<String, HashSet<String>>,
    ) -> Self {
        let proportions = BuscoProportions {
            complete: summary.results.complete,
            fragmented: summary.results.fragmented,
            missing: summary.results.missing,
            duplicated: summary.results.multi_copy,
            single: summary.results.single_copy,
        };
        let counts = BuscoCounts {
            complete: by_status.get("complete").map_or(0, |s| s.len()),
            fragmented: by_status.get("fragmented").map_or(0, |s| s.len()),
            missing: by_status.get("missing").map_or(0, |s| s.len()),
            duplicated: by_status.get("duplicated").map_or(0, |s| s.len()),
            single: by_status.get("single").map_or(0, |s| s.len()),
        };
        let lists = BuscoLists {
            complete: by_status
                .get("complete")
                .map_or(vec![], |s| s.iter().cloned().collect()),
            fragmented: by_status
                .get("fragmented")
                .map_or(vec![], |s| s.iter().cloned().collect()),
            missing: by_status
                .get("missing")
                .map_or(vec![], |s| s.iter().cloned().collect()),
            duplicated: by_status
                .get("duplicated")
                .map_or(vec![], |s| s.iter().cloned().collect()),
            single: by_status
                .get("single")
                .map_or(vec![], |s| s.iter().cloned().collect()),
        };
        let count = summary.lineage_dataset.number_of_buscos;
        Self::new(
            taxon_id,
            assembly_id,
            lineage,
            count,
            proportions,
            lists,
            counts,
        )
    }

    pub fn header(&self) -> String {
        format!(
            "{}\t{}\t{}_{}\t{}_{}\t{}_{}\t{}_{}\t{}_{}\t{}_{}\t{}_{}\t{}_{}\t{}_{}\t{}_{}\t{}_{}\t{}_{}\t{}_{}\t{}_{}\t{}_{}\t{}_{}",
            "taxon_id",
            "assembly_id",
            &self.lineage,
            "count",
            &self.lineage,
            "complete_proportion",
            &self.lineage,
            "fragmented_proportion",
            &self.lineage,
            "missing_proportion",
            &self.lineage,
            "duplicated_proportion",
            &self.lineage,
            "single_proportion",
            &self.lineage,
            "complete_count",
            &self.lineage,
            "fragmented_count",
            &self.lineage,
            "missing_count",
            &self.lineage,
            "duplicated_count",
            &self.lineage,
            "single_count",
            &self.lineage,
            "complete",
            &self.lineage,
            "fragmented",
            &self.lineage,
            "missing",
            &self.lineage,
            "duplicated",
            &self.lineage,
            "single",
        )
    }

    pub fn to_string(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.taxon_id,
            self.assembly_id,
            self.count,
            self.proportions.complete,
            self.proportions.fragmented,
            self.proportions.missing,
            self.proportions.duplicated,
            self.proportions.single,
            self.counts.complete,
            self.counts.fragmented,
            self.counts.missing,
            self.counts.duplicated,
            self.counts.single,
            self.lists.complete.join(","),
            self.lists.fragmented.join(","),
            self.lists.missing.join(","),
            self.lists.duplicated.join(","),
            self.lists.single.join(","),
        )
    }

    pub fn to_ghubs_config(&self) -> GHubsConfig {
        let mut attributes = HashMap::new();
        attributes.insert(
            "taxon_id".to_string(),
            GHubsFieldConfig {
                header: Some(StringOrVec::Single("taxon_id".to_string())),
                field_type: FieldType::Keyword,
                ..Default::default()
            },
        );
        attributes.insert(
            "assembly_id".to_string(),
            GHubsFieldConfig {
                header: Some(StringOrVec::Single("assembly_id".to_string())),
                field_type: FieldType::Keyword,
                ..Default::default()
            },
        );
        let lineage = self.lineage.to_lowercase();
        for status in vec!["complete", "fragmented", "missing", "duplicated", "single"] {
            let list_config = BuscoLists::to_field_config(lineage.clone(), status.to_string());
            if let Some(header) = &list_config.header {
                attributes.insert(header.to_string(), list_config);
            }
            let count_config = BuscoCounts::to_field_config(lineage.clone(), status.to_string());
            if let Some(header) = &count_config.header {
                attributes.insert(header.to_string(), count_config);
            }
            let proportion_config =
                BuscoProportions::to_field_config(lineage.clone(), status.to_string());
            if let Some(header) = &proportion_config.header {
                attributes.insert(header.to_string(), proportion_config);
            }
        }
        let file = GHubsFileConfig {
            format: GHubsFileFormat::TSV,
            header: true,
            name: PathBuf::from(format!("{}.busco.tsv", self.lineage)),
            source_name: Some("BlobToolKit".to_string()),
            source_stub: Some("https://blobtoolkit.genomehubs.org".to_string()),
            ..Default::default()
        };
        let mut identifiers = HashMap::new();
        identifiers.insert(
            "assembly_id".to_string(),
            GHubsFieldConfig {
                header: Some(StringOrVec::Single("assembly_id".to_string())),
                field_type: FieldType::Keyword,
                constraint: Some(ConstraintConfig {
                    len: Some(32),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        let mut taxonomy = HashMap::new();
        taxonomy.insert(
            "taxon_id".to_string(),
            GHubsFieldConfig {
                header: Some(StringOrVec::Single("taxon_id".to_string())),
                field_type: FieldType::Keyword,
                ..Default::default()
            },
        );

        GHubsConfig {
            file: Some(file),
            identifiers: Some(identifiers),
            taxonomy: Some(taxonomy),
            attributes: Some(attributes),
            ..Default::default()
        }
    }

    pub fn to_file(&self, file_path: &PathBuf) -> Result<(), anyhow::Error> {
        let mut file_writer = get_file_writer(file_path, false);
        let header = self.header();
        writeln!(file_writer, "{}", header)?;
        let data = self.to_string();
        writeln!(file_writer, "{}", data)?;
        file_writer.flush()?;
        Ok(())
    }
}

fn parse_busco(
    taxon_id: String,
    assembly_id: String,
    busco_dir: &PathBuf,
    sequences: &HashMap<String, &Feature>,
    busco_count: Option<usize>,
) -> Result<(BuscoStats, Features), anyhow::Error> {
    let mut features = Vec::new();

    let span = sequences.values().map(|f| f.length).sum::<usize>();
    // extract lineage from busco_dir matching pattern (\w+_odb\d+)
    // the lineage may end with _metaeuk and this should be discarded when setting the lineage variable
    let regex = regex::Regex::new(r"(\w+_odb\d+)(_metaeuk|_augustus)?").unwrap();
    // If busco_dir ends with "full_table.tsv" or "full_table.tsv.gz", use its parent directory
    let mut full_table_name = "full_table.tsv.gz";
    let (busco_dir_name, mut busco_dir_path) = {
        let file_name = busco_dir.file_name().unwrap().to_str().unwrap();
        if file_name == "full_table.tsv" || file_name == "full_table.tsv.gz" {
            full_table_name = file_name;
            (
                busco_dir
                    .parent()
                    .unwrap()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
                busco_dir.parent().unwrap().to_path_buf(),
            )
        } else {
            (file_name.to_string(), busco_dir.clone())
        }
    };
    let lineage;
    if let Some(captures) = regex.captures(&busco_dir_name) {
        lineage = captures.get(1).unwrap().as_str().to_string();
        // if second capture group then add a subdirectory to busco_dir_path
        if let Some(_subdir) = captures.get(2) {
            busco_dir_path = busco_dir_path.join(format!("run_{}", lineage));
        }
    } else {
        return Err(anyhow::anyhow!(
            "No matching lineage found in busco_dir: {:?}",
            busco_dir
        ));
    };
    let full_table_reader = get_csv_reader(
        &Some(busco_dir_path.join(full_table_name)),
        b'\t',
        true,
        None,
        2,
        true,
    );
    let by_status = parse_busco_full_table(
        sequences,
        &mut features,
        span,
        lineage.clone(),
        full_table_reader,
    );

    let summary_path = busco_dir_path.join("short_summary.json");
    let summary = match file_reader(summary_path.clone()) {
        Ok(file) => {
            let summary_reader = std::io::BufReader::new(file);
            match serde_json::from_reader(summary_reader) {
                Ok(summary) => Ok(summary),
                Err(_) => {
                    // Try reading short_summary.txt instead
                    let txt_path = busco_dir_path.join("short_summary.txt");
                    match file_reader(txt_path) {
                        Ok(txt_file) => {
                            let txt_reader = std::io::BufReader::new(txt_file);
                            parse_busco_txt_summary(txt_reader)
                        }
                        Err(e) => Err(anyhow::anyhow!("Failed to read summary.txt: {}", e)),
                    }
                }
            }
        }
        Err(_) => {
            // Try reading short_summary.txt instead
            let txt_path = busco_dir_path.join("short_summary.txt");
            match file_reader(txt_path) {
                Ok(txt_file) => {
                    let txt_reader = std::io::BufReader::new(txt_file);
                    parse_busco_txt_summary(txt_reader)
                }
                Err(e) => Err(anyhow::anyhow!("Failed to read summary.txt: {}", e)),
            }
        }
    }?;

    // Custom parser for BUSCO txt summary format
    fn parse_busco_txt_summary<R: BufRead>(mut reader: R) -> Result<BuscoSummary, anyhow::Error> {
        use regex::Regex;
        let mut version = String::new();
        let mut lineage_name = String::new();
        let mut lineage_creation_date = String::new();
        let mut lineage_number_of_buscos = 0usize;
        let mut lineage_number_of_species = 0usize;
        let mut hmmsearch = String::new();
        let mut metaeuk = String::new();
        let mut summary_line = String::new();
        let mut n_markers = 0usize;
        let mut complete = 0.0;
        let mut single_copy = 0.0;
        let mut multi_copy = 0.0;
        let mut fragmented = 0.0;
        let mut missing = 0.0;
        let mut domain = String::new();
        let mut number_of_scaffolds = 0usize;
        let mut number_of_contigs = 0usize;
        let mut total_length = 0usize;
        let mut percent_gaps = 0.0;
        let mut scaffold_n50 = 0usize;
        let mut contigs_n50 = 0usize;

        let re_version = Regex::new(r"^# BUSCO version is: (.+)").unwrap();
        let re_lineage = Regex::new(r"^# The lineage dataset is: ([^(]+) \(Creation date: ([^,]+), number of genomes: (\d+), number of BUSCOs: (\d+)\)").unwrap();
        let re_summary =
            Regex::new(r"C:([\d.]+)%\[S:([\d.]+)%,D:([\d.]+)%\],F:([\d.]+)%,M:([\d.]+)%,n:(\d+)")
                .unwrap();
        let re_hmmsearch = Regex::new(r"hmmsearch: ([^\s]+)").unwrap();
        let re_metaeuk = Regex::new(r"metaeuk: ([^\s]+)").unwrap();

        for line in reader.lines() {
            let line = line?;
            if line.starts_with('#') {
                if let Some(caps) = re_version.captures(&line) {
                    version = caps[1].to_string();
                }
                if let Some(caps) = re_lineage.captures(&line) {
                    lineage_name = caps[1].trim().to_string();
                    lineage_creation_date = caps[2].trim().to_string();
                    lineage_number_of_species = caps[3].parse().unwrap_or(0);
                    lineage_number_of_buscos = caps[4].parse().unwrap_or(0);
                }
            } else if line.contains("Results:") || line.trim().is_empty() {
                continue;
            } else if re_summary.is_match(&line) {
                summary_line = line.trim().to_string();
                if let Some(caps) = re_summary.captures(&line) {
                    complete = caps[1].parse().unwrap_or(0.0);
                    single_copy = caps[2].parse().unwrap_or(0.0);
                    multi_copy = caps[3].parse().unwrap_or(0.0);
                    fragmented = caps[4].parse().unwrap_or(0.0);
                    missing = caps[5].parse().unwrap_or(0.0);
                    n_markers = caps[6].parse().unwrap_or(0);
                }
            } else if let Some(caps) = re_hmmsearch.captures(&line) {
                hmmsearch = caps[1].to_string();
            } else if let Some(caps) = re_metaeuk.captures(&line) {
                metaeuk = caps[1].to_string();
            }
        }

        // Populate BuscoResults
        let results = BuscoResults {
            complete,
            single_copy,
            multi_copy,
            fragmented,
            missing,
            n_markers,
            domain: lineage_name.clone(),
            number_of_scaffolds,
            number_of_contigs,
            total_length,
            percent_gaps,
            scaffold_n50,
            contigs_n50,
        };
        let lineage_dataset = BuscoLineageDataset {
            name: lineage_name,
            creation_date: lineage_creation_date,
            number_of_buscos: lineage_number_of_buscos,
            number_of_species: lineage_number_of_species,
        };
        let versions = BuscoVersions {
            hmmsearch,
            bbtools: String::new(),
            metaeuk,
            busco: version,
        };
        Ok(BuscoSummary {
            results,
            lineage_dataset,
            versions,
        })
    }

    let busco_stats = BuscoStats::from_summary(
        taxon_id.clone(),
        assembly_id.clone(),
        lineage.clone(),
        summary,
        by_status.clone(),
    );

    Ok((
        busco_stats,
        Features {
            taxon_id,
            assembly_id,
            window_size: 1.0,
            busco_count,
            features,
        },
    ))
}

fn parse_busco_full_table(
    sequences: &HashMap<String, &Feature>,
    features: &mut Vec<Feature>,
    span: usize,
    lineage: String,
    full_table_reader: csv::Reader<Box<dyn BufRead>>,
) -> HashMap<String, HashSet<String>> {
    let mut complete = HashSet::new();
    let mut fragmented = HashSet::new();
    let mut missing = HashSet::new();
    let mut duplicated = HashSet::new();
    let mut single = HashSet::new();
    // parse the full_table.tsv file
    for record in parse_full_table(full_table_reader) {
        if let Ok((id, status, score, sequence, start, end, strand, length)) = record {
            // if sequence ends with :\d+-\d+ then remove it
            let sequence = sequence.split(':').next().unwrap_or(&sequence).to_string();
            let seq_feature = sequences.get(&sequence).unwrap();
            let midpoint = (start + end) / 2;
            let midpoint_proportion = midpoint as f64 / seq_feature.length as f64;
            let seq_proportion = length as f64 / span as f64;
            let feature = Feature::new(
                format!("{}:{}-{}:{}", sequence, start, end, &id),
                sequence,
                vec![
                    format!("{}-busco-gene", lineage),
                    "busco-gene".to_string(),
                    "gene".to_string(),
                ]
                .join(","),
                start,
                end,
                strand,
                length,
                None,
                None,
                None,
                midpoint,
                midpoint_proportion,
                seq_proportion,
                Some(id.clone()),
                seq_feature.name.clone(),
                Some(score),
                Some(status.clone()),
                None,
            );
            features.push(feature);
            match status.as_str() {
                "Complete" => {
                    complete.insert(id.clone());
                    single.insert(id.clone());
                }
                "Fragmented" => {
                    fragmented.insert(id.clone());
                }
                "Duplicated" => {
                    duplicated.insert(id.clone());
                }
                "Missing" => {
                    missing.insert(id.clone());
                }
                _ => {}
            }
        } else if let Err(e) = record {
            let id = e.to_string();
            if id.contains(": Missing") {
                let id = id.split(":").next().unwrap_or("").to_string();
                missing.insert(id);
            }
        }
    }
    let mut by_status = HashMap::new();
    by_status.insert("complete".to_string(), complete);
    by_status.insert("fragmented".to_string(), fragmented);
    by_status.insert("missing".to_string(), missing);
    by_status.insert("duplicated".to_string(), duplicated);
    by_status.insert("single".to_string(), single);
    by_status
}

#[derive(Debug, Deserialize)]
pub struct DatasetsSequenceReport {
    // assembly_accession: String,
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
    pub fn to_feature(&self) -> Feature {
        let feature_id = self.genbank_accession.clone();
        let sequence_id = self.genbank_accession.clone();
        let mut feature_type = match self.role.as_str() {
            "assembled-molecule" => self
                .assigned_molecule_location_type
                .to_string()
                .to_lowercase(),
            "unplaced-scaffold" => "scaffold".to_string(),
            "unlocalized-scaffold" => "scaffold".to_string(),
            "unlocalized-contig" => "contig".to_string(),
            _ => "contig".to_string(),
        };
        feature_type.push_str(",sequence,toplevel");
        let start = 1;
        let end = self.length;
        let strand = 1;
        let length = self.length;
        let gc = match self.gc_percent {
            Some(gc_percent) => Some(gc_percent / 100.0),
            None => None,
        };
        let coverage = None;
        let masked = None;
        let midpoint = length / 2;
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
        let score = None;
        let status = None;
        let busco_counts = None;
        Feature {
            feature_id,
            sequence_id,
            feature_type,
            start,
            end,
            strand,
            length,
            gc,
            coverage,
            masked,
            midpoint,
            midpoint_proportion,
            seq_proportion,
            name: name.clone(),
            sequence_name: name,
            score,
            status,
            busco_counts,
        }
    }
}

fn parse_datasets_sequence_report(
    accession: &str,
    taxon_id: Option<String>,
) -> Result<Features, anyhow::Error> {
    if Command::new("datasets").output().is_err() {
        return Err(anyhow::anyhow!("datasets is not installed"));
    }

    let output = Command::new("datasets")
        .args(&[
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
        return Err(anyhow::anyhow!(
            "Error fetching sequences report: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let json_lines = String::from_utf8(output.stdout)?;
    let mut features = Vec::new();

    for line in json_lines.lines() {
        let record: DatasetsSequenceReport = serde_json::from_str(line)?;
        let feature = record.to_feature();
        features.push(feature);
    }

    Ok(Features {
        taxon_id: taxon_id.unwrap_or("None".to_string()),
        assembly_id: accession.to_string(),
        window_size: 1.0,
        busco_count: None,
        features,
    })
}

#[derive(Debug, Deserialize)]
struct BlobToolKitSearch {
    // accession: String,
    // alias: String,
    // bioproject: String,
    // biosample: String,
    // prefix: String,
    // class: String,
    // family: String,
    // genus: String,
    // kingdom: String,
    // name: String,
    // order: String,
    // phylum: String,
    // superkingdom: String,
    // taxid: String,
    // taxon_name: String,
    // revision: u8,
    id: String,
}

#[derive(Debug, Deserialize)]
struct BlobToolKitSearchResults {
    results: Vec<BlobToolKitSearch>,
}

fn find_blobtoolkit_url(accession: &str) -> Option<PathBuf> {
    let blobtoolkit_url = format!(
        "https://blobtoolkit.genomehubs.org/api/v1/search/{}",
        accession
    );
    // fetch the blobtoolkit url and parse the json
    if let Some(output) = Command::new("curl")
        .args(&["-s", &blobtoolkit_url])
        .output()
        .ok()
    {
        if !output.status.success() {
            return None;
        }

        if let Some(json) = String::from_utf8(output.stdout).ok() {
            let wrapped_json = format!("{{\"results\":{}}}", json);
            if let Ok(search_results) =
                serde_json::from_str::<BlobToolKitSearchResults>(&wrapped_json)
            {
                if search_results.results.len() == 0 {
                    return None;
                }
                let blobtoolkit_id = &search_results.results[0].id;
                let blobtoolkit_path = PathBuf::from(format!(
                    "https://blobtoolkit.genomehubs.org/api/v1/dataset/id/{}",
                    blobtoolkit_id
                ));
                return Some(blobtoolkit_path);
            } else {
                return None;
            }
        };
    }
    None
}

fn lookup_goat_lineages(taxon_id: String) -> Result<Vec<String>, anyhow::Error> {
    // use curl to fetch directly from the API
    let url = format!(
        "https://goat.genomehubs.org/api/v2/search?query=tax_lineage%28{}%29&result=taxon&fields=odb10_lineage&includeEstimates=true&taxonomy=ncbi",
        taxon_id
    );
    let output = Command::new("curl").args(&["-s", &url]).output()?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Error fetching lineages: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let lineages = String::from_utf8(output.stdout)?;
    let lineages: serde_json::Value = serde_json::from_str(&lineages)?;
    let lineages = lineages["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|result| {
            result["result"]["fields"]["odb10_lineage"]["value"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect::<Vec<String>>();

    Ok(lineages)
}

fn process_busco_dirs(
    busco_dirs: &Vec<PathBuf>,
    taxon_id: String,
) -> Result<Vec<PathBuf>, anyhow::Error> {
    let mut processed_busco_dirs = Vec::new();
    // if only one busco dir is provided and it does not end with \w+_odb\d+,
    // assume it is a directory containing multiple busco results
    // lookup the set of relevant lineages using goat and add to busco_dirs
    if busco_dirs.len() == 1 {
        let busco_dir = &busco_dirs[0];
        if let Some(busco_dir_name) = busco_dir.file_name() {
            let regex = regex::Regex::new(r"(\w+_odb\d+)").unwrap();
            if !regex.is_match(busco_dir_name.to_str().unwrap()) {
                let lineages = lookup_goat_lineages(taxon_id)?;
                for lineage in lineages {
                    let lineage_dir = busco_dir.join(lineage);
                    // if lineage_dir.exists() {
                    processed_busco_dirs.push(lineage_dir);
                    // }
                }
            } else {
                processed_busco_dirs.push(busco_dir.clone());
            }
        }
    } else {
        processed_busco_dirs = busco_dirs.clone();
    }
    Ok(processed_busco_dirs)
}

/// Execute the `index` subcommand from `blobtk`.
pub fn index(options: &cli::IndexOptions) -> Result<(), anyhow::Error> {
    if options.schema {
        let schema = schema_for!(GHubsConfig);
        let mut writer = get_writer(&options.out);

        writeln!(&mut writer, "{}", to_string_pretty(&schema).unwrap())?;
    }
    let mut sequences = HashMap::new();
    let mut contig_values = Features {
        ..Default::default()
    };
    let mut busco_count = None;
    let mut accession = options.datasets_accession.clone();
    let mut taxon_id = options.taxon_id.clone();
    let mut optional_blobdir_path = options.blobdir.clone();
    if let Some(datasets_accession) = &accession {
        if optional_blobdir_path.is_none() {
            optional_blobdir_path = find_blobtoolkit_url(datasets_accession);
        }
        if optional_blobdir_path.is_none() && options.taxon_id.is_none() {
            return Err(anyhow::anyhow!(
                "No BlobToolKit URL found for {}. Please provide a taxon_id",
                datasets_accession
            ));
        }
        // log Parsing datasets sequence report
        log::info!("Parsing datasets sequence report");
        log::info!("Taxon ID: {:?}", options.taxon_id);
        log::info!("Datasets accession: {:?}", datasets_accession);
        contig_values =
            parse_datasets_sequence_report(datasets_accession, options.taxon_id.clone())?;
    }
    if let Some(blobdir_path) = optional_blobdir_path {
        // log Parsing blobdir
        log::info!("Parsing blobdir");
        log::info!("Blobdir path: {:?}", blobdir_path);
        let meta = blobdir::parse_blobdir(&blobdir_path)?;
        if accession.is_none() {
            accession = Some(meta.assembly.accession.clone());
        }
        if taxon_id.is_none() {
            taxon_id = Some(meta.taxon.taxid.clone());
        }
        if contig_values.features.is_empty() {
            contig_values = per_contig_values(&meta, &blobdir_path)?;
            contig_values.to_file(&options.out)?;
        } else {
            let btk_contig_values = per_contig_values(&meta, &blobdir_path)?;
            // add busco counts from btk_contig_values to contig_values
            // match on sequence_id
            for feature in &mut contig_values.features {
                if let Some(btk_feature) = btk_contig_values
                    .features
                    .iter()
                    .find(|f| f.sequence_id == feature.sequence_id)
                {
                    feature.busco_counts = btk_feature.busco_counts.clone();
                }
            }
            contig_values.taxon_id = btk_contig_values.taxon_id;
            contig_values.to_file(&options.out)?;
        }

        for feature in &contig_values.features {
            sequences.insert(feature.sequence_id.clone(), feature);
        }

        for window in &options.window_size {
            if window == &1.0 {
                continue;
            }
            // log Parsing window
            log::info!("Parsing window");
            log::info!("Window size: {:?}", window);
            let window_values = per_window_values(&meta, &blobdir_path, &sequences, window)?;
            let _window_analysis = window_analysis(&meta, window);
            window_values.append_to_file(&options.out)?;
        }
        busco_count = match meta.busco_list.as_ref() {
            Some(busco_list) => Some(busco_list.len()),
            None => None,
        };
    }
    if !contig_values.features.is_empty() {
        let yaml_path = options.out.as_ref().unwrap().with_extension("types.yaml");
        let analysis = GHubsAnalysisConfig {
            analysis_id: format!(
                "assembly-{}",
                accession.clone().unwrap_or("None".to_string())
            ),
            assembly_id: match accession {
                Some(ref acc) => Some(StringOrVec::Single(acc.clone())),
                None => None,
            },
            taxon_id: match taxon_id {
                Some(ref taxid) => Some(StringOrVec::Single(taxid.clone())),
                None => None,
            },
            description: Some(format!(
                "Public assembly {}",
                accession.clone().unwrap_or("None".to_string())
            )),
            name: "assembly".to_string(),
            title: Some(format!(
                "Public assembly {}",
                accession.clone().unwrap_or("None".to_string())
            )),
        };
        let file = options.out.clone();
        let feature_config = contig_values.to_ghubs_config(file, Some(analysis));
        feature_config.write_yaml(&yaml_path)?;

        if sequences.is_empty() {
            for feature in &contig_values.features {
                sequences.insert(feature.sequence_id.clone(), feature);
            }
        }
        let accession = accession.unwrap_or("None".to_string());
        let taxon_id = contig_values.taxon_id.clone();

        if let Some(busco_dirs) = &options.busco {
            let processed_busco_dirs = process_busco_dirs(busco_dirs, taxon_id.clone())?;
            let out_dir = options.out.as_ref().unwrap().parent().unwrap();
            // log Parsing busco
            log::info!("Parsing busco");
            for busco_dir in processed_busco_dirs {
                log::info!("Busco dir: {:?}", busco_dir);
                let (busco_stats, busco_values) = parse_busco(
                    taxon_id.clone(),
                    accession.clone(),
                    &busco_dir,
                    &sequences,
                    busco_count,
                )?;
                let accession_dir = out_dir.join(&accession);
                std::fs::create_dir_all(&accession_dir)?;
                let lineage_file =
                    accession_dir.join(format!("{}.busco.tsv", busco_stats.lineage.to_lowercase()));
                let lineage_yaml_file = accession_dir.join(format!(
                    "{}.busco.types.yaml",
                    busco_stats.lineage.to_lowercase()
                ));
                let lineage_config = busco_stats.to_ghubs_config();
                lineage_config
                    .write_yaml(&lineage_yaml_file)
                    .expect("Failed to write lineage config to file");
                busco_stats.to_file(&lineage_file)?;
                busco_values.append_to_file(&options.out)?;
            }
        }
    }
    Ok(())
}
