//!
//! Invoked by calling:
//! `blobtk index <args>`

use std::collections::HashMap;
use std::io::BufRead;
use std::option;
use std::path::PathBuf;

use anyhow;
use numfmt::Formatter;
use numfmt::Precision;
use schemars::schema_for;
use serde_json::to_string_pretty;

use crate::blobdir;
use crate::blobdir::Field;
use crate::cli;
use crate::io::get_csv_reader;
use crate::io::get_file_writer;
use crate::io::get_writer;
use crate::parse::genomehubs::FieldType;
use crate::parse::genomehubs::GHubsConfig;
use crate::parse::genomehubs::GHubsFieldConfig;
use crate::parse::genomehubs::StringOrVec;

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
            score,
            status,
            busco_counts,
        }
    }

    pub fn to_string(&self, busco_count: Option<usize>) -> String {
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
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.feature_id,
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
            score_str,
            self.status.as_ref().unwrap_or(&"None".to_string()),
            busco_counts_str
        )
    }

    pub fn to_header(&self) -> String {
        let mut header = "feature_id\tsequence_id\tfeature_type\tstart\tend\tstrand\tlength\tgc\tcoverage\tmasked\tmidpoint\tmidpoint_proportion\tseq_proportion\tname\tscore\tstatus".to_string();
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

#[derive(Debug)]
pub struct Features {
    pub window_size: f64,
    pub busco_count: Option<usize>,
    pub features: Vec<Feature>,
}

impl Features {
    pub fn new(window_size: f64, features: Vec<Feature>, busco_count: Option<usize>) -> Self {
        Self {
            window_size,
            features,
            busco_count,
        }
    }

    pub fn from_vecs(
        feature_type: String,
        ids: Vec<String>,
        lengths: Vec<usize>,
        strands: Option<Vec<i8>>,
        gcs: Option<Vec<f64>>,
        coverages: Option<Vec<f64>>,
        maskeds: Option<Vec<f64>>,
        names: Option<Vec<String>>,
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
        Self::new(1.0, features, busco_count)
    }

    pub fn from_vec_of_vecs(
        window_size: f64,
        feature_type: String,
        ids: Vec<String>,
        lengths: Vec<Vec<usize>>,
        strands: Option<Vec<Vec<i8>>>,
        gcs: Option<Vec<Vec<f64>>>,
        coverages: Option<Vec<Vec<f64>>>,
        maskeds: Option<Vec<Vec<f64>>>,
        names: Option<Vec<Vec<String>>>,
        scores: Option<Vec<Vec<f64>>>,
        statuses: Option<Vec<Vec<String>>>,
        busco_counts: Option<HashMap<String, Vec<Vec<usize>>>>,
    ) -> Self {
        let mut features = Vec::new();
        for (i, id) in ids.iter().enumerate() {
            let mut start = 1;
            let span = lengths[i].iter().sum::<usize>();
            for (j, length) in lengths[i].iter().enumerate() {
                let length = length.clone();
                let end = start + length - 1;
                let feature_id = format!("{}:{}-{}:{}", id, start, end, feature_type);
                let sequence_id = id.clone();
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
        Self::new(window_size, features, busco_count)
    }

    pub fn to_string(&self) -> String {
        let mut output = Vec::new();
        for feature in &self.features {
            output.push(feature.to_string(self.busco_count));
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

    pub fn to_ghubs_config(&self) -> GHubsConfig {
        let mut attributes = HashMap::new();
        let fields = vec![
            ("feature_id", FieldType::Keyword, None),
            ("feature_type", FieldType::Keyword, Some(",")),
            ("name", FieldType::Keyword, Some(",")),
            ("sequence_id", FieldType::Keyword, None),
            ("sequence_name", FieldType::Keyword, Some(",")),
            ("analysis_name", FieldType::Keyword, None),
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
        let config = GHubsConfig {
            attributes: Some(attributes),
            ..Default::default()
        };

        config
    }
}

fn per_contig_values(
    meta: &blobdir::Meta,
    blobdir_path: &PathBuf,
) -> Result<Features, anyhow::Error> {
    let plot_meta = meta.plot.clone();
    let identifiers = blobdir::parse_field_identifiers("identifiers".to_string(), &blobdir_path)?;
    let gc_values = blobdir::parse_field_float("gc".to_string(), &blobdir_path)?;
    let length_values = blobdir::parse_field_int("length".to_string(), &blobdir_path)?;
    let coverage_values = if let Some(coverage) = plot_meta.y {
        blobdir::parse_field_float(coverage, &blobdir_path)?
    } else {
        vec![0.0; length_values.len()]
    };
    let masked_values = blobdir::parse_field_float("masked".to_string(), &blobdir_path)?;
    let busco_counts = if let Some(busco_list) = &meta.busco_list {
        let mut _busco_counts = HashMap::new();
        for busco in busco_list {
            let field_id = format!("{}_count", busco.2);
            let busco_values = blobdir::parse_field_int(field_id.clone(), &blobdir_path)?;
            _busco_counts.insert(field_id.clone(), busco_values);
        }
        Some(_busco_counts)
    } else {
        None
    };
    let features = Features::from_vecs(
        "chromosome".to_string(),
        identifiers,
        length_values,
        None,
        Some(gc_values),
        Some(coverage_values),
        Some(masked_values),
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
    contig_values: &Features,
    window_size: &f64,
) -> Result<Features, anyhow::Error> {
    let plot_meta = meta.plot.clone();

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
    let masked_values = blobdir::parse_field_float_windows(
        get_window_id("masked", window_size),
        &blobdir_path,
        None,
    )?;
    let busco_counts = if let Some(busco_list) = &meta.busco_list {
        let mut _busco_counts = HashMap::new();
        for busco in busco_list {
            let field_name = format!("{}_count", busco.2);
            let field_id = get_window_id(&field_name, window_size);
            let busco_values =
                blobdir::parse_field_int_windows(field_id.clone(), &blobdir_path, None)?;
            _busco_counts.insert(field_name, busco_values.0);
        }
        Some(_busco_counts)
    } else {
        None
    };
    let features = Features::from_vec_of_vecs(
        *window_size,
        format!("window-{}", window_size),
        identifiers,
        length_values.0,
        None,
        Some(gc_values.0),
        coverage_values,
        Some(masked_values.0),
        None,
        None,
        None,
        busco_counts,
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

fn busco_analysis(meta: &blobdir::Meta, busco: &(String, usize, String)) -> Analysis {
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

fn parse_busco(
    meta: &blobdir::Meta,
    busco_dir: &PathBuf,
    sequences: &HashMap<String, &Feature>,
    _busco_count: usize,
) -> Result<Features, anyhow::Error> {
    let mut features = Vec::new();
    if let Some(busco_list) = &meta.busco_list {
        let span = sequences.values().map(|f| f.length).sum::<usize>();
        for busco in busco_list {
            // if third value in busco tuple is in the busco_dir name
            // then parse the busco_dir
            if busco_dir
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .contains(&busco.2)
            {
                let busco_analysis = busco_analysis(&meta, &busco);
                let busco_count = busco.1;
                // find the full_table.tsv file in the busco_dir
                let full_table_reader = get_csv_reader(
                    &Some(busco_dir.join("full_table.tsv.gz")),
                    b'\t',
                    true,
                    None,
                    2,
                );
                // parse the full_table.tsv file
                for record in parse_full_table(full_table_reader) {
                    if let Ok((id, status, score, sequence, start, end, strand, length)) = record {
                        let seq_feature = sequences.get(&sequence).unwrap();
                        let midpoint = (start + end) / 2;
                        let midpoint_proportion = midpoint as f64 / seq_feature.length as f64;
                        let seq_proportion = length as f64 / span as f64;
                        let feature = Feature::new(
                            format!("{}:{}-{}:{}", sequence, start, end, &id),
                            sequence,
                            "busco".to_string(),
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
                            Some(id),
                            Some(score),
                            Some(status),
                            None,
                        );
                        features.push(feature);
                    }
                }
            }
        }
    }
    Ok(Features {
        window_size: 1.0,
        busco_count: Some(_busco_count),
        features,
    })
}

/// Execute the `index` subcommand from `blobtk`.
pub fn index(options: &cli::IndexOptions) -> Result<(), anyhow::Error> {
    if options.schema {
        let schema = schema_for!(GHubsConfig);
        let mut writer = get_writer(&options.out);

        writeln!(&mut writer, "{}", to_string_pretty(&schema).unwrap())?;
    }
    if let Some(blobdir_path) = &options.blobdir {
        let meta = blobdir::parse_blobdir(blobdir_path)?;
        let contig_values = per_contig_values(&meta, blobdir_path)?;
        let yaml_path = options.out.as_ref().unwrap().with_extension("yaml");
        contig_values.to_ghubs_config().write_yaml(&yaml_path)?;
        let mut sequences = HashMap::new();
        for feature in &contig_values.features {
            sequences.insert(feature.sequence_id.clone(), feature);
        }
        contig_values.to_file(&options.out)?;

        for window in &options.window_size {
            if window == &1.0 {
                continue;
            }
            let window_values = per_window_values(&meta, blobdir_path, &contig_values, window)?;
            let window_analysis = window_analysis(&meta, window);
            window_values.append_to_file(&options.out)?;
        }
        if let Some(busco_dirs) = &options.busco {
            let busco_count = meta.busco_list.as_ref().unwrap().len();
            for busco_dir in busco_dirs {
                let busco_values = parse_busco(&meta, &busco_dir, &sequences, busco_count)?;
                busco_values.append_to_file(&options.out)?;
            }
        }
    }
    Ok(())
}
