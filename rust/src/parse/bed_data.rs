//! Parser to read a set of BED files and extract the relevant information for indexing into Elasticsearch.
//! Bed files must have 1 line per 1kb
//! Parser merges the BED files into a single set of features and summarises the 1kb segments in a set of window sizes
//! The module uses the Feature struct to represent the attributes and metadata of each feature, and provides functions to parse the BED files and extract the relevant information into a vector of Feature structs. The module also includes error handling to ensure that any issues encountered during parsing are properly reported and handled.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error;
use crate::index::es::models::documents::FeatureDocument;
use crate::index::es::models::nested_documents::NestedAttribute;
use crate::io;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "lowercase")]
pub enum SummaryFunction {
    Count,
    Min,
    Max,
    Sum,
    Mean,
    Median,
    Mode,
    StdDev,
    #[serde(alias = "subwindow_variance")]
    SubWindowVariance {
        size: usize,
    }, // number of sub-windows to calculate variance over
}

impl SummaryFunction {
    // Returns a highly efficient function pointer
    fn get_calculator(&self) -> fn(&[f64]) -> f64 {
        match self {
            SummaryFunction::Count => |data| data.len() as f64,
            SummaryFunction::Min => |data| data.iter().copied().fold(f64::NAN, f64::min),
            SummaryFunction::Max => |data| data.iter().copied().fold(f64::NAN, f64::max),
            SummaryFunction::Sum => |data| data.iter().sum::<f64>(),
            SummaryFunction::Mean => |data| {
                if data.is_empty() {
                    return f64::NAN;
                }
                data.iter().sum::<f64>() / data.len() as f64
            },
            SummaryFunction::Median => |data| {
                if data.is_empty() {
                    return f64::NAN;
                }
                let mut sorted = data.to_vec();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let mid = sorted.len() / 2;
                if sorted.len() % 2 == 0 {
                    (sorted[mid - 1] + sorted[mid]) / 2.0
                } else {
                    sorted[mid]
                }
            },
            SummaryFunction::Mode => |data| {
                if data.is_empty() {
                    return f64::NAN;
                }
                // Note: Mode on f64 requires grouping by bits due to NaN/precision issues
                let mut counts = std::collections::HashMap::new();
                for &val in data {
                    *counts.entry(val.to_bits()).or_insert(0) += 1;
                }
                counts
                    .into_iter()
                    .max_by_key(|&(_, count)| count)
                    .map(|(bits, _)| f64::from_bits(bits))
                    .unwrap_or(f64::NAN)
            },
            SummaryFunction::StdDev => |data| {
                if data.len() < 2 {
                    return f64::NAN;
                }
                let mean = data.iter().sum::<f64>() / data.len() as f64;
                let variance = data
                    .iter()
                    .map(|&value| {
                        let diff = mean - value;
                        diff * diff
                    })
                    .sum::<f64>()
                    / (data.len() - 1) as f64; // Sample standard deviation
                variance.sqrt()
            },
            SummaryFunction::SubWindowVariance { size: _sub_windows } => |data| {
                let k = 100;
                let num_lines = data.len();
                if num_lines < k {
                    return f64::NAN;
                }
                // chunk size (ceil)
                let chunk_size = ((num_lines as f64) / (k as f64)).ceil() as usize;
                let mut means = Vec::with_capacity(k);
                let mut start = 0usize;
                for _ in 0..k {
                    if start >= num_lines {
                        break;
                    }
                    let end = std::cmp::min(start + chunk_size, num_lines);
                    let slice = &data[start..end];
                    if slice.is_empty() {
                        break;
                    }
                    let sum: f64 = slice.iter().copied().filter(|v| !v.is_nan()).sum();
                    let count = slice.iter().filter(|v| !v.is_nan()).count();
                    if count == 0 {
                        means.push(f64::NAN);
                    } else {
                        means.push(sum / count as f64);
                    }
                    start = end;
                }
                if means.len() < k / 2 || means.iter().any(|m| m.is_nan()) {
                    return f64::NAN;
                }
                let mean_of_means: f64 = means.iter().sum::<f64>() / k as f64;
                // sample variance
                let var = means
                    .iter()
                    .map(|m| {
                        let d = m - mean_of_means;
                        d * d
                    })
                    .sum::<f64>()
                    / ((k - 1) as f64);
                var
            },
        }
    }

    // Optional helper to execute the function directly
    fn compute(&self, data: &[f64]) -> f64 {
        (self.get_calculator())(data)
    }

    fn name(&self) -> String {
        match self {
            SummaryFunction::Count => "count".to_string(),
            SummaryFunction::Min => "min".to_string(),
            SummaryFunction::Max => "max".to_string(),
            SummaryFunction::Sum => "sum".to_string(),
            SummaryFunction::Mean => "mean".to_string(),
            SummaryFunction::Median => "median".to_string(),
            SummaryFunction::Mode => "mode".to_string(),
            SummaryFunction::StdDev => "stddev".to_string(),
            SummaryFunction::SubWindowVariance { size: _n } => "var".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValueColumn {
    pub label: String,
    pub index: usize,
    #[serde(rename = "type")]
    pub value_type: String,
    pub summary_functions: Vec<SummaryFunction>,
}

impl ValueColumn {
    pub fn name(&self, index: usize) -> String {
        if index == 0 {
            self.label.clone()
        } else {
            format!("{}_{}", self.label, self.summary_functions[index].name())
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BedConfig {
    pub path: PathBuf,
    pub value_columns: Vec<ValueColumn>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WindowSpec {
    Size { size: usize },
    Proportion { proportion: f64 },
}

impl WindowSpec {
    pub fn to_string(&self) -> String {
        match self {
            WindowSpec::Size { size } => {
                // format size with si suffix
                let si_size = if *size >= 1_000_000_000 {
                    format!("{}G", size / 1_000_000_000)
                } else if *size >= 1_000_000 {
                    format!("{}M", size / 1_000_000)
                } else if *size >= 1_000 {
                    format!("{}k", size / 1_000)
                } else {
                    format!("{}", size)
                };
                format!("win-{}", si_size)
            }
            WindowSpec::Proportion { proportion } => format!("win-{:.2}", proportion),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MultiBedConfig {
    pub accession: String,
    pub taxon_id: String,
    pub lines_per_unit: usize,
    #[serde(rename = "files")]
    pub bed_configs: Vec<BedConfig>,
    #[serde(rename = "windows")]
    pub window_specs: Vec<WindowSpec>,
}

#[derive(Clone, Debug)]
pub struct AccumulatorColumn {
    pub label: String,
    pub count: usize,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub values: Option<Vec<f64>>,
    pub summary_functions: Vec<SummaryFunction>,
}

impl AccumulatorColumn {
    pub fn new(label: String, summary_functions: Vec<SummaryFunction>) -> Self {
        AccumulatorColumn {
            label,
            count: 0,
            sum: 0.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            values: Some(vec![]),
            summary_functions,
        }
    }

    pub fn add(&mut self, value: f64) {
        self.count += 1;
        // skip NaN values for min/max/sum calculations
        if value.is_nan() {
            return;
        }
        self.sum += value;
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
        if let Some(ref mut vals) = self.values {
            vals.push(value);
        }
    }

    pub fn finish(&mut self) -> HashMap<SummaryFunction, f64> {
        let mut result = HashMap::new();
        if self.count > 0 {
            for func in &self.summary_functions {
                match func {
                    SummaryFunction::Count => {
                        result.insert(SummaryFunction::Count, self.count as f64)
                    }
                    SummaryFunction::Min => result.insert(SummaryFunction::Min, self.min),
                    SummaryFunction::Max => result.insert(SummaryFunction::Max, self.max),
                    SummaryFunction::Sum => result.insert(SummaryFunction::Sum, self.sum),
                    SummaryFunction::Mean => {
                        result.insert(SummaryFunction::Mean, self.sum / self.count as f64)
                    }
                    SummaryFunction::Median | SummaryFunction::Mode | SummaryFunction::StdDev => {
                        if let Some(ref vals) = self.values {
                            let value = func.compute(vals);
                            result.insert(func.clone(), value)
                        } else {
                            None
                        }
                    }
                    SummaryFunction::SubWindowVariance { size: _size } => {
                        if let Some(ref vals) = self.values {
                            let value = func.compute(vals);
                            result.insert(func.clone(), value)
                        } else {
                            None
                        }
                    }
                };
            }
        }
        result
    }
}

#[derive(Clone, Debug)]
pub struct Accumulator {
    columns: Vec<AccumulatorColumn>,
    start: Option<usize>,
    end: Option<usize>,
}

impl Accumulator {
    pub fn new(value_columns: &[ValueColumn]) -> Self {
        let columns = value_columns
            .iter()
            .map(|vc| AccumulatorColumn::new(vc.label.clone(), vc.summary_functions.clone()))
            .collect();
        Accumulator {
            columns,
            start: None,
            end: None,
        }
    }

    pub fn reset(&mut self) {
        for col in &mut self.columns {
            col.count = 0;
            col.sum = 0.0;
            col.min = f64::INFINITY;
            col.max = f64::NEG_INFINITY;
            col.values = match col.values {
                Some(_) => Some(vec![]),
                None => None,
            };
        }
        self.start = None;
        self.end = None;
    }
}

#[derive(Clone, Debug)]
pub struct Feature {
    pub sequence_id: String,
    pub start: usize,
    pub end: usize,
    pub values: Vec<f64>,
}

pub fn set_window_name(
    sequence_id: &str,
    start: usize,
    end: usize,
    window_spec: &WindowSpec,
) -> String {
    format!(
        "{}:{}-{}:{}",
        sequence_id,
        start,
        end,
        window_spec.to_string()
    )
}

pub fn parse_bed_line(line: &str, value_columns: &[ValueColumn]) -> Result<Feature, error::Error> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() < 3 {
        return Err(error::Error::ParseError(format!(
            "Invalid BED line: {}",
            line
        )));
    }

    let sequence_id = fields[0].to_string();
    let start = fields[1].parse::<usize>().map_err(|e| {
        error::Error::ParseError(format!(
            "Invalid start position in BED line: {}: {}",
            line, e
        ))
    })?;
    let end = fields[2].parse::<usize>().map_err(|e| {
        error::Error::ParseError(format!("Invalid end position in BED line: {}: {}", line, e))
    })?;

    let mut values = Vec::new();
    for value_column in value_columns {
        if value_column.index >= fields.len() {
            return Err(error::Error::ParseError(format!(
                "Value column index {} out of bounds for BED line: {}",
                value_column.index, line
            )));
        }
        let value_str = fields[value_column.index];
        let value = match value_column.value_type.as_str() {
            "int" => value_str.parse::<i64>().map_err(|e| {
                error::Error::ParseError(format!("Invalid int value in BED line: {}: {}", line, e))
            })? as f64, // Convert int to float for consistency
            "float" => value_str.parse::<f64>().map_err(|e| {
                error::Error::ParseError(format!(
                    "Invalid float value in BED line: {}: {}",
                    line, e
                ))
            })?,
            _ => {
                return Err(error::Error::UnsupportedFileType(
                    value_column.value_type.clone(),
                ))
            }
        };
        values.push(value);
    }

    Ok(Feature {
        sequence_id,
        start,
        end,
        values,
    })
}

pub fn parse_bed_files(
    config: &MultiBedConfig,
) -> Result<HashMap<String, FeatureDocument>, error::Error> {
    let mut features: Vec<Feature> = Vec::new();
    let mut feature_docs: HashMap<String, FeatureDocument> = HashMap::new();
    // let lines_per_unit = config.lines_per_unit;

    let windows = &config.window_specs;
    let windowed_features: Vec<Vec<Feature>> = windows.iter().map(|_| Vec::new()).collect();

    for bed_config in &config.bed_configs {
        let mut bed_file = io::file_reader(bed_config.path.clone())?;
        // read the BED file line by line and parse each line into a Feature struct
        let bed_reader = &mut *bed_file;
        let mut accumulators: Vec<Accumulator> = Vec::new();
        for _window_spec in &config.window_specs {
            let value_columns = &bed_config.value_columns;
            accumulators.push(Accumulator::new(value_columns));
        }
        let mut per_seq_buffers: HashMap<String, Vec<Feature>> = HashMap::new();
        for line in bed_reader.lines() {
            let line = line.map_err(|e| {
                error::Error::ReaderError(format!(
                    "Error reading line from BED file {}: {}",
                    bed_config.path.display(),
                    e
                ))
            })?;
            let feature = parse_bed_line(&line, &bed_config.value_columns)?;
            per_seq_buffers
                .entry(feature.sequence_id.clone())
                .or_insert_with(Vec::new)
                .push(feature.clone());
        }
        for (seq_id, buffer) in per_seq_buffers {
            let sequence_length = buffer.last().map_or(0, |f| f.end);
            for window_spec in config.window_specs.iter() {
                let lines_per_window = match window_spec {
                    WindowSpec::Size { size } => *size / config.lines_per_unit,
                    WindowSpec::Proportion { proportion } => {
                        let total_lines = buffer.len();
                        ((total_lines as f64) * proportion).ceil() as usize
                    }
                };
                let mut acc = Accumulator::new(&bed_config.value_columns);
                let last_index = buffer.len() - 1;
                for (fi, feature) in &mut buffer.iter().enumerate() {
                    if acc.start.is_none() {
                        acc.start = Some(feature.start);
                    }
                    acc.end = Some(feature.end);
                    for (i, &value) in feature.values.iter().enumerate() {
                        acc.columns[i].add(value);
                    }
                    // if window is full or this is the last line
                    if acc.columns[0].count >= lines_per_window || fi == last_index {
                        let window_name = set_window_name(
                            &seq_id,
                            acc.start.unwrap_or(0),
                            acc.end.unwrap_or(0),
                            window_spec,
                        );
                        if !feature_docs.contains_key(&window_name) {
                            let doc = FeatureDocument::new(
                                window_name.clone(),
                                Some(seq_id.clone()),
                                window_spec.to_string(),
                                acc.start.unwrap_or(0),
                                acc.end.unwrap_or(0),
                                None, // strand
                                None, // container_ids
                                seq_id.clone(),
                                sequence_length,
                                config.accession.clone(),
                                config.taxon_id.clone(),
                                None, // ancestors
                                None, // file_id
                                None, // analysis_id
                            );
                            feature_docs.insert(window_name.clone(), doc);
                        }
                        let doc = feature_docs.get_mut(&window_name).unwrap();
                        for (index, col) in acc.columns.iter_mut().enumerate() {
                            let summary = col.finish();
                            for (fi, sf) in &mut col.summary_functions.iter().enumerate() {
                                // use the config value column name
                                let name = bed_config.value_columns[index].name(fi);
                                let attribute = NestedAttribute {
                                    key: name.clone(),
                                    half_float_value: Some(
                                        summary.get(sf).copied().unwrap_or(f64::NAN) as f32,
                                    ),
                                    ..Default::default()
                                };
                                if doc.attributes.is_none() {
                                    doc.attributes = Some(vec![]);
                                }
                                doc.attributes.as_mut().unwrap().push(attribute);
                            }
                            // let values: Vec<f64> = col
                            //     .summary_functions
                            //     .iter()
                            //     .map(|sf| summary.get(sf).copied().unwrap_or(f64::NAN))
                            //     .collect();
                            // summarised_values.push(values);
                        }

                        acc.reset();
                    }
                }
            }
        }
    }

    Ok(feature_docs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bed_line() {
        let value_columns = vec![
            ValueColumn {
                label: "value1".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Mean],
            },
            ValueColumn {
                label: "value2".to_string(),
                index: 4,
                value_type: "int".to_string(),
                summary_functions: vec![SummaryFunction::Sum],
            },
        ];
        let line = "chr1\t100\t200\t1.23\t4";
        let feature = parse_bed_line(line, &value_columns).unwrap();
        assert_eq!(feature.sequence_id, "chr1");
        assert_eq!(feature.start, 100);
        assert_eq!(feature.end, 200);
        assert_eq!(feature.values, vec![1.23, 4.0]);
    }

    // temporary test for parse_bed_files function
    #[test]
    fn test_parse_bed_files() {
        let bed_config_gc = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k1/GCA_016920705.1.GC.1k.bedGraph.gz"),
            value_columns: vec![ValueColumn {
                label: "gc".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Mean,SummaryFunction::SubWindowVariance { size: 100 }],
            }],
        };
        let bed_config_n = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k1/GCA_016920705.1.N.1k.bedGraph.gz"),
            value_columns: vec![ValueColumn {
                label: "n".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Count, SummaryFunction::Mean, SummaryFunction::Sum],
            }],
        };
        let bed_config_at_skew = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k1/GCA_016920705.1.AT_skew.1k.bedGraph.gz"),
            value_columns: vec![ValueColumn {
                label: "at_skew".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Count, SummaryFunction::Mean, SummaryFunction::Sum],
            }],
        };
        let bed_config_gc_skew = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k1/GCA_016920705.1.GC_skew.1k.bedGraph.gz"),
            value_columns: vec![ValueColumn {
                label: "gc_skew".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Count, SummaryFunction::Mean, SummaryFunction::Sum],
            }],
        };
        let bed_config_shannon = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k1/GCA_016920705.1.nucShannon.1k.bedGraph.gz"),
            value_columns: vec![ValueColumn {
                label: "nucShannon".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Count, SummaryFunction::Mean, SummaryFunction::Sum],
            }],
        };
        let bed_config_cpg = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k2/GCA_016920705.1.CpG.1k.bedGraph.gz"),
            value_columns: vec![ValueColumn {
                label: "cpg".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Mean, SummaryFunction::SubWindowVariance { size: 100 }],
            }],
        };

        let multi_bed_config = MultiBedConfig {
            accession: "GCA_016920705.1".to_string(),
            taxon_id: "1518534".to_string(),
            lines_per_unit: 1000,
            bed_configs: vec![
                bed_config_gc,
                bed_config_n,
                bed_config_at_skew,
                bed_config_gc_skew,
                bed_config_shannon,
                bed_config_cpg,
            ],
            window_specs: vec![WindowSpec::Size { size: 1000000 }],
        };
        let features = parse_bed_files(&multi_bed_config).unwrap();
        let json_features = serde_json::to_string_pretty(&features).unwrap();
        // print the json_features to stdout for inspection
        println!("{}", &json_features);
        // dbg!(&json_features);
        assert_eq!(features.len(), 4);
        // assert_eq!(features[0].sequence_id, "CM029348.1");
        // assert_eq!(features[0].start, 0);
        // assert_eq!(features[0].end, 1000);
        // assert_eq!(features[0].values, vec![0.4, 0.1]);
    }
}
