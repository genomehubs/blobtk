//!
//! Invoked by calling:
//! `blobtk index <args>`

use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;

use anyhow;
use needletail::sequence;
use schemars::schema_for;
use serde_json::to_string_pretty;

use crate::blobdir;
use crate::cli;
use crate::io::get_csv_reader;
use crate::io::get_writer;
use crate::parse::genomehubs::GHubsConfig;

pub use cli::IndexOptions;

#[derive(Debug)]
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
        }
    }
}

#[derive(Debug)]
pub struct Features {
    pub window_size: f64,
    pub features: Vec<Feature>,
}

impl Features {
    pub fn new(window_size: f64, features: Vec<Feature>) -> Self {
        Self {
            window_size,
            features,
        }
    }

    pub fn from_vecs(
        feature_type: String,
        names: Vec<String>,
        lengths: Vec<usize>,
        strands: Option<Vec<i8>>,
        gcs: Option<Vec<f64>>,
        coverages: Option<Vec<f64>>,
        maskeds: Option<Vec<f64>>,
    ) -> Self {
        let mut features = Vec::new();
        let span = lengths.iter().sum::<usize>();
        for (i, name) in names.iter().enumerate() {
            let feature_id = format!("{}:{}", name, feature_type);
            let sequence_id = name.clone();
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
            });
        }
        Self::new(1.0, features)
    }

    pub fn from_vec_of_vecs(
        window_size: f64,
        feature_type: String,
        names: Vec<String>,
        lengths: Vec<Vec<usize>>,
        strands: Option<Vec<Vec<i8>>>,
        gcs: Option<Vec<Vec<f64>>>,
        coverages: Option<Vec<Vec<f64>>>,
        maskeds: Option<Vec<Vec<f64>>>,
    ) -> Self {
        let mut features = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let mut start = 1;
            let span = lengths[i].iter().sum::<usize>();
            for (j, length) in lengths[i].iter().enumerate() {
                let length = length.clone();
                let end = start + length - 1;
                let feature_id = format!("{}:{}-{}:{}", name, start, end, feature_type);
                let sequence_id = name.clone();
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
                });
                start += length;
            }
        }
        Self::new(window_size, features)
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
    let features = Features::from_vecs(
        "chromosome".to_string(),
        identifiers,
        length_values,
        None,
        Some(gc_values),
        Some(coverage_values),
        Some(masked_values),
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
    let mut float_fields = vec!["gc".to_string(), "masked".to_string()];
    match meta.plot.clone().y {
        Some(coverage) => float_fields.push(coverage.clone()),
        None => (),
    }
    let int_fields = vec!["position".to_string()];
    for field in float_fields {
        let window_id = get_window_id(&field, window_size);
        let window_values = blobdir::parse_field_float_windows(window_id, &blobdir_path, None)?;
        // dbg!(&window_values);
    }
    for field in int_fields {
        let window_id = get_window_id(&field, window_size);
        let window_values = blobdir::parse_field_int_windows(window_id, &blobdir_path, None)?;
        dbg!(&window_values);
    }

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
    let features = Features::from_vec_of_vecs(
        *window_size,
        format!("window-{}", window_size),
        identifiers,
        length_values.0,
        None,
        Some(gc_values.0),
        coverage_values,
        Some(masked_values.0),
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

fn parse_busco(
    meta: &blobdir::Meta,
    busco_dir: &PathBuf,
    sequences: &HashMap<String, &Feature>,
) -> Result<(), anyhow::Error> {
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
                    let (id, status, score, sequence, start, end, strand, length) = record?;
                    let seq_feature = sequences.get(&sequence).unwrap();
                    let midpoint = (start + end) / 2;
                    let midpoint_proportion = midpoint as f64 / seq_feature.length as f64;
                    let seq_proportion = length as f64 / span as f64;
                    let feature = Feature::new(
                        id,
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
                    );
                    dbg!(feature);
                }

                // let busco_values = blobdir::parse_field_float("score".to_string(), busco_dir)?;
                dbg!(busco_count);
            }
        }
    }
    dbg!(busco_dir);
    Ok(())
}

/// Execute the `index` subcommand from `blobtk`.
pub fn index(options: &cli::IndexOptions) -> Result<(), anyhow::Error> {
    if options.schema {
        dbg!("testing");
        let schema = schema_for!(GHubsConfig);
        let mut writer = get_writer(&options.out);

        writeln!(&mut writer, "{}", to_string_pretty(&schema).unwrap())?;
    }
    if let Some(blobdir_path) = &options.blobdir {
        let meta = blobdir::parse_blobdir(blobdir_path)?;
        let contig_values = per_contig_values(&meta, blobdir_path)?;
        dbg!(&contig_values);
        let mut sequences = HashMap::new();
        for feature in &contig_values.features {
            sequences.insert(feature.sequence_id.clone(), feature);
        }

        // for window in &options.window_size {
        //     if window == &1.0 {
        //         continue;
        //     }
        //     let window_values = per_window_values(&meta, blobdir_path, &contig_values, window)?;
        // }
        if let Some(busco_dirs) = &options.busco {
            for busco_dir in busco_dirs {
                let busco_values = parse_busco(&meta, &busco_dir, &sequences)?;
                dbg!(busco_values);
            }
        }
    }
    Ok(())
}
