//! Parser to read a set of BED files and extract the relevant information for indexing into Elasticsearch.
//! Bed files must have 1 line per 1kb
//! Parser merges the BED files into a single set of features and summarises the 1kb segments in a set of window sizes
//! The module uses the Feature struct to represent the attributes and metadata of each feature, and provides functions to parse the BED files and extract the relevant information into a vector of Feature structs. The module also includes error handling to ensure that any issues encountered during parsing are properly reported and handled.

use std::collections::HashMap;
use std::io::{copy, BufRead, Cursor, Read};
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
            let label = format!("{}_{}", self.label, self.summary_functions[index].name());
            label
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BedConfig {
    pub path: PathBuf,
    pub local_path: Option<PathBuf>,
    pub value_columns: Vec<ValueColumn>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RemnantPolicy {
    Trailing,    // current behavior: leftover at chromosome end
    Centered,    // center the remnant by anchoring at both ends
    Symmetric,   // split the remnant evenly at both ends
    Discard,     // discard the remnant entirely
    Distributed, // distribute the remnant across the entire sequence
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WindowSpec {
    Size {
        size: usize,
        remnant_policy: RemnantPolicy,
    },
    Proportion {
        proportion: f64,
    },
}

impl WindowSpec {
    pub fn to_string(&self) -> String {
        match self {
            WindowSpec::Size {
                size,
                remnant_policy: _,
            } => {
                // format size with si suffix
                let si_size = if *size >= 1_000_000_000 {
                    let s = format!("{}G", size / 1_000_000_000);
                    s
                } else if *size >= 1_000_000 {
                    let s = format!("{}M", size / 1_000_000);
                    s
                } else if *size >= 1_000 {
                    let s = format!("{}k", size / 1_000);
                    s
                } else {
                    let s = format!("{}", size);
                    s
                };
                let s = format!("win-{}", si_size);
                s
            }
            WindowSpec::Proportion { proportion } => {
                let s = format!("win-{:.2}", proportion);
                s
            }
        }
    }
}

pub fn window_spec_cache_key(window_spec: &WindowSpec) -> String {
    match window_spec {
        WindowSpec::Size {
            size,
            remnant_policy,
        } => {
            format!("size:{}:{:?}", size, remnant_policy)
        }
        WindowSpec::Proportion { proportion } => {
            format!("proportion:{}", proportion)
        }
    }
}

pub fn window_bounds_for_sequence(
    sequence_length: usize,
    window_spec: &WindowSpec,
    _lines_per_unit: usize,
) -> Vec<(usize, usize)> {
    let mut bounds = Vec::new();

    match window_spec {
        WindowSpec::Size {
            size,
            remnant_policy,
        } => {
            if *size == 0 {
                return bounds;
            }

            let full_bins = sequence_length / *size;
            let remnant = sequence_length % *size;

            match remnant_policy {
                RemnantPolicy::Trailing => {
                    let mut start = 0;
                    while start < sequence_length {
                        let end = (start + *size).min(sequence_length);
                        bounds.push((start, end));
                        start = end;
                    }
                }
                RemnantPolicy::Discard => {
                    let mut start = 0;
                    while start + *size <= sequence_length {
                        let end = start + *size;
                        bounds.push((start, end));
                        start = end;
                    }
                }
                RemnantPolicy::Distributed => {
                    let num_windows = if sequence_length == 0 {
                        0
                    } else {
                        (sequence_length as f64 / *size as f64).ceil() as usize
                    };
                    if num_windows == 0 {
                        return bounds;
                    }

                    let mut start = 0;
                    for i in 0..num_windows {
                        let end = if i + 1 == num_windows {
                            sequence_length
                        } else {
                            ((sequence_length * (i + 1)) + num_windows - 1) / num_windows
                        };
                        bounds.push((start, end));
                        start = end;
                    }
                }
                RemnantPolicy::Centered => {
                    if sequence_length == 0 {
                        return bounds;
                    }
                    if full_bins == 0 {
                        bounds.push((0, sequence_length));
                        return bounds;
                    }
                    if remnant == 0 {
                        let mut start = 0;
                        for _ in 0..full_bins {
                            let end = (start + *size).min(sequence_length);
                            bounds.push((start, end));
                            start = end;
                        }
                        return bounds;
                    }

                    let mut start = 0;
                    let central_index = full_bins / 2;
                    for i in 0..full_bins {
                        let fixed_end = start + *size;
                        if i == central_index {
                            let end = (fixed_end + remnant).min(sequence_length);
                            bounds.push((start, end));
                            start = end;
                        } else {
                            bounds.push((start, fixed_end));
                            start = fixed_end;
                        }
                    }

                    if start < sequence_length {
                        bounds.push((start, sequence_length));
                    }
                }
                RemnantPolicy::Symmetric => {
                    if sequence_length == 0 {
                        return bounds;
                    }
                    if full_bins == 0 {
                        bounds.push((0, sequence_length));
                        return bounds;
                    }
                    if remnant == 0 {
                        let mut start = 0;
                        for _ in 0..full_bins {
                            let end = (start + *size).min(sequence_length);
                            bounds.push((start, end));
                            start = end;
                        }
                        return bounds;
                    }

                    let left_extra = remnant / 2;
                    let right_extra = remnant - left_extra;
                    let mut start = 0;

                    if full_bins > 0 {
                        let left_window_end = (*size + left_extra).min(sequence_length);
                        bounds.push((0, left_window_end));
                        start = left_window_end;
                    }

                    for _ in 1..(full_bins.saturating_sub(1)) {
                        let end = (start + *size).min(sequence_length);
                        bounds.push((start, end));
                        start = end;
                    }

                    if full_bins > 1 {
                        let right_window_start =
                            (sequence_length - (*size + right_extra)).max(start);
                        if right_window_start > start {
                            bounds.push((start, right_window_start));
                        }
                        bounds.push((right_window_start, sequence_length));
                    } else {
                        bounds.push((start, sequence_length));
                    }
                }
            }
        }
        WindowSpec::Proportion { proportion } => {
            if *proportion <= 0.0 || sequence_length == 0 {
                return bounds;
            }
            let window_size = (sequence_length as f64 * proportion).ceil() as usize;
            let mut start = 0;
            while start < sequence_length {
                let end = (start + window_size).min(sequence_length);
                bounds.push((start, end));
                start = end;
            }
        }
    }

    bounds
}

pub fn window_bounds_for_sequence_cached(
    sequence_id: &str,
    sequence_length: usize,
    window_spec: &WindowSpec,
    lines_per_unit: usize,
    cache: &mut HashMap<String, Vec<(usize, usize)>>,
) -> Vec<(usize, usize)> {
    let key = format!(
        "{}:{}:{}",
        sequence_id,
        sequence_length,
        window_spec_cache_key(window_spec)
    );
    if let Some(bounds) = cache.get(&key) {
        return bounds.clone();
    }
    let bounds = window_bounds_for_sequence(sequence_length, window_spec, lines_per_unit);
    cache.insert(key, bounds.clone());
    bounds
}

pub fn window_index_for_position(bounds: &[(usize, usize)], position: usize) -> Option<usize> {
    bounds
        .iter()
        .position(|(start, end)| position >= *start && position < *end)
}

pub fn window_ids_for_range(
    sequence_id: &str,
    sequence_length: usize,
    feat_start_1based: usize,
    feat_end_1based: usize,
    window_spec: &WindowSpec,
    lines_per_unit: usize,
    bounds_cache: &mut HashMap<String, Vec<(usize, usize)>>,
) -> Vec<String> {
    let start_0based = feat_start_1based.saturating_sub(1);
    let end_0based = feat_end_1based.max(feat_start_1based).saturating_sub(1);
    let bounds = window_bounds_for_sequence_cached(
        sequence_id,
        sequence_length,
        window_spec,
        lines_per_unit,
        bounds_cache,
    );

    let first_idx = window_index_for_position(&bounds, start_0based).unwrap_or(0);
    let last_idx =
        window_index_for_position(&bounds, end_0based).unwrap_or(bounds.len().saturating_sub(1));

    (first_idx..=last_idx)
        .filter_map(|idx| {
            let (w_start, w_end) = *bounds.get(idx)?;
            Some(set_window_name(sequence_id, w_start, w_end, window_spec))
        })
        .collect()
}

pub fn window_ids_for_midpoint(
    sequence_id: &str,
    sequence_length: usize,
    feat_start_1based: usize,
    feat_end_1based: usize,
    window_spec: &WindowSpec,
    lines_per_unit: usize,
    bounds_cache: &mut HashMap<String, Vec<(usize, usize)>>,
) -> Vec<String> {
    let start_0based = feat_start_1based.saturating_sub(1);
    let end_0based = feat_end_1based.max(feat_start_1based).saturating_sub(1);
    let midpoint = if end_0based >= start_0based {
        start_0based + ((end_0based - start_0based) / 2)
    } else {
        start_0based
    };

    let bounds = window_bounds_for_sequence_cached(
        sequence_id,
        sequence_length,
        window_spec,
        lines_per_unit,
        bounds_cache,
    );

    window_index_for_position(&bounds, midpoint)
        .map(|idx| {
            let (w_start, w_end) = bounds[idx];
            vec![set_window_name(sequence_id, w_start, w_end, window_spec)]
        })
        .unwrap_or_default()
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
    let s = format!(
        "{}:{}-{}:{}",
        sequence_id,
        start,
        end,
        window_spec.to_string()
    );
    s
}

pub fn parse_bed_line(line: &str, value_columns: &[ValueColumn]) -> Result<Feature, error::Error> {
    let clean = line.trim_end_matches('\r');
    let fields: Vec<&str> = clean.split('\t').collect();
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

fn fill_per_seq_buffers(
    bed_reader: &mut dyn std::io::BufRead,
    bed_config: &BedConfig,
    bed_path: &std::path::Path,
) -> Result<HashMap<String, Vec<Feature>>, error::Error> {
    let mut per_seq_buffers: HashMap<String, Vec<Feature>> = HashMap::new();
    for line in bed_reader.lines() {
        let line = line.map_err(|e| {
            error::Error::ReaderError(format!(
                "Error reading line from BED file {}: {}",
                bed_path.display(),
                e
            ))
        })?;
        let feature = match parse_bed_line(&line, &bed_config.value_columns) {
            Ok(f) => f,
            Err(e) => {
                return Err(error::Error::ParseError(format!(
                    "Failed to parse BED file: {}, line: {}: {}",
                    bed_path.display(),
                    line,
                    e
                )));
            }
        };
        per_seq_buffers
            .entry(feature.sequence_id.clone())
            .or_insert_with(Vec::new)
            .push(feature.clone());
    }
    Ok(per_seq_buffers)
}

pub fn read_bed_file(config: &BedConfig) -> Result<HashMap<String, Vec<Feature>>, error::Error> {
    // read from local of available. If not read from remote using the provided path. If local path provided but not exists then write remote to local after reading
    if let Some(local) = &config.local_path {
        let maybe_bed_file = io::file_reader(local.clone());
        if let Ok(mut bed_file) = maybe_bed_file {
            let bed_reader = &mut *bed_file;
            return fill_per_seq_buffers(bed_reader, config, local);
        } else {
            // read from remote if local file is not available
            let remote_bed_file = io::file_reader(config.path.clone());
            if let Ok(mut bed_file) = remote_bed_file {
                let mut bytes = Vec::new();
                Read::read_to_end(&mut *bed_file, &mut bytes)?;

                let mut parse_reader = Cursor::new(bytes.clone());
                let per_seq_buffers = fill_per_seq_buffers(&mut parse_reader, config, &config.path);

                let mut local_file = io::get_file_writer(local, false);
                copy(&mut Cursor::new(bytes), &mut *local_file)?;

                return per_seq_buffers;
            } else {
                return Err(error::Error::ReaderError(format!(
                    "Failed to open remote BED file {}: {}",
                    config.path.display(),
                    "File not found"
                )));
            }
        }
    } else if let Ok(mut bed_file) = io::file_reader(config.path.clone()) {
        let bed_reader = &mut *bed_file;
        return fill_per_seq_buffers(bed_reader, config, &config.path);
    } else {
        return Err(error::Error::ReaderError(format!(
            "Failed to open BED file {}: {}",
            config.path.display(),
            "File not found"
        )));
    }
}

pub fn parse_bed_files(
    config: &MultiBedConfig,
) -> Result<HashMap<String, FeatureDocument>, error::Error> {
    let mut feature_docs: HashMap<String, FeatureDocument> = HashMap::new();

    for bed_config in &config.bed_configs {
        if let Ok(per_seq_buffers) = read_bed_file(bed_config) {
            for (seq_id, buffer) in per_seq_buffers {
                let sequence_length = buffer.last().map_or(0, |f| f.end);
                for window_spec in config.window_specs.iter() {
                    let bounds = window_bounds_for_sequence(
                        sequence_length,
                        window_spec,
                        config.lines_per_unit,
                    );
                    if bounds.is_empty() {
                        continue;
                    }

                    for (window_start, window_end) in bounds {
                        let mut acc = Accumulator::new(&bed_config.value_columns);
                        let mut saw_overlap = false;

                        for feature in &buffer {
                            if feature.start >= window_end || feature.end <= window_start {
                                continue;
                            }
                            saw_overlap = true;
                            acc.start = Some(window_start);
                            acc.end = Some(window_end);
                            for (index, &value) in feature.values.iter().enumerate() {
                                acc.columns[index].add(value);
                            }
                        }

                        if !saw_overlap {
                            continue;
                        }

                        let window_name =
                            set_window_name(&seq_id, window_start, window_end, window_spec);
                        if !feature_docs.contains_key(&window_name) {
                            let doc = FeatureDocument::new(
                                window_name.clone(),
                                Some(seq_id.clone()),
                                window_spec.to_string(),
                                window_start,
                                window_end,
                                None,
                                None,
                                seq_id.clone(),
                                sequence_length,
                                config.accession.clone(),
                                config.taxon_id.clone(),
                                None,
                                None,
                                None,
                            );
                            feature_docs.insert(window_name.clone(), doc);
                        }

                        let doc = feature_docs.get_mut(&window_name).unwrap();
                        for (index, col) in acc.columns.iter_mut().enumerate() {
                            let summary = col.finish();
                            for (fi, sf) in col.summary_functions.iter().enumerate() {
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
                        }

                        let attributes = doc.attributes.as_mut().unwrap();
                        attributes.push(NestedAttribute {
                            key: "assembly_id".to_string(),
                            keyword_value: Some(super::genomehubs::StringOrVec::Single(
                                config.accession.clone(),
                            )),
                            ..Default::default()
                        });
                        attributes.push(NestedAttribute {
                            key: "taxon_id".to_string(),
                            keyword_value: Some(super::genomehubs::StringOrVec::Single(
                                config.taxon_id.clone(),
                            )),
                            ..Default::default()
                        });
                        attributes.push(NestedAttribute {
                            key: "sequence_id".to_string(),
                            keyword_value: Some(super::genomehubs::StringOrVec::Single(
                                seq_id.clone(),
                            )),
                            ..Default::default()
                        });
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

    #[test]
    fn test_window_bounds_for_sequence_symmetric_remnant() {
        let bounds = window_bounds_for_sequence(
            10,
            &WindowSpec::Size {
                size: 4,
                remnant_policy: RemnantPolicy::Symmetric,
            },
            1,
        );
        assert_eq!(bounds, vec![(0, 5), (5, 10)]);
    }

    #[test]
    fn test_window_bounds_for_sequence_centered_remnant() {
        let bounds = window_bounds_for_sequence(
            14,
            &WindowSpec::Size {
                size: 4,
                remnant_policy: RemnantPolicy::Centered,
            },
            1,
        );
        assert_eq!(bounds, vec![(0, 4), (4, 10), (10, 14)]);
    }

    #[test]
    fn test_window_id_for_midpoint_uses_midpoint_not_range_overlap() {
        let mut cache = HashMap::new();
        let window_ids = window_ids_for_midpoint(
            "chr1",
            10,
            2,
            8,
            &WindowSpec::Size {
                size: 4,
                remnant_policy: RemnantPolicy::Centered,
            },
            1,
            &mut cache,
        );
        assert_eq!(window_ids, vec!["chr1:4-10:win-4".to_string()]);
    }

    // temporary test for parse_bed_files function
    #[test]
    fn test_parse_bed_files_creates_final_partial_window() {
        let tmp = std::env::temp_dir().join("blobtk_bed_window_regression.bed");
        std::fs::write(
            &tmp,
            "chr1\t0\t1000\t0.1\nchr1\t1000\t2000\t0.2\nchr1\t2000\t3000\t0.3\n",
        )
        .unwrap();

        let cfg = MultiBedConfig {
            accession: "GCA_test".to_string(),
            taxon_id: "123".to_string(),
            lines_per_unit: 1000,
            bed_configs: vec![BedConfig {
                path: tmp,
                local_path: None,
                value_columns: vec![ValueColumn {
                    label: "gc".to_string(),
                    index: 3,
                    value_type: "float".to_string(),
                    summary_functions: vec![SummaryFunction::Mean],
                }],
            }],
            window_specs: vec![WindowSpec::Size {
                size: 2000,
                remnant_policy: RemnantPolicy::Trailing,
            }],
        };

        let docs = parse_bed_files(&cfg).unwrap();
        assert!(!docs.is_empty());
        assert!(docs.keys().any(|id| id.contains("chr1:0-2000:win-2k")));
        assert!(docs.keys().any(|id| id.contains("chr1:2000-3000:win-2k")));
    }

    #[test]
    fn test_parse_bed_files_respects_centered_remnant_policy() {
        let tmp = std::env::temp_dir().join("blobtk_bed_centered_window_regression.bed");
        std::fs::write(
            &tmp,
            "chr1\t0\t1000\t0.1\nchr1\t1000\t2000\t0.2\nchr1\t2000\t3000\t0.3\nchr1\t3000\t4000\t0.4\nchr1\t4000\t5000\t0.5\n",
        )
        .unwrap();

        let cfg = MultiBedConfig {
            accession: "GCA_test".to_string(),
            taxon_id: "123".to_string(),
            lines_per_unit: 1000,
            bed_configs: vec![BedConfig {
                path: tmp,
                local_path: None,
                value_columns: vec![ValueColumn {
                    label: "gc".to_string(),
                    index: 3,
                    value_type: "float".to_string(),
                    summary_functions: vec![SummaryFunction::Mean],
                }],
            }],
            window_specs: vec![WindowSpec::Size {
                size: 2000,
                remnant_policy: RemnantPolicy::Centered,
            }],
        };

        let docs = parse_bed_files(&cfg).unwrap();
        assert!(docs.keys().any(|id| id.contains("chr1:0-2000:win-2k")));
        assert!(docs.keys().any(|id| id.contains("chr1:2000-5000:win-2k")));
        assert!(!docs.keys().any(|id| id.contains("chr1:4000-5000:win-2k")));
    }

    #[test]
    fn test_parse_bed_files_creates_windows_with_window_feature_type() {
        let tmp = std::env::temp_dir().join("blobtk_bed_window_feature_type.bed");
        std::fs::write(
            &tmp,
            "chr1\t0\t1000\t0.1\nchr1\t1000\t2000\t0.2\nchr1\t2000\t3000\t0.3\n",
        )
        .unwrap();

        let cfg = MultiBedConfig {
            accession: "GCA_test".to_string(),
            taxon_id: "123".to_string(),
            lines_per_unit: 1000,
            bed_configs: vec![BedConfig {
                path: tmp,
                local_path: None,
                value_columns: vec![ValueColumn {
                    label: "gc".to_string(),
                    index: 3,
                    value_type: "float".to_string(),
                    summary_functions: vec![SummaryFunction::Mean],
                }],
            }],
            window_specs: vec![
                WindowSpec::Size {
                    size: 2000,
                    remnant_policy: RemnantPolicy::Trailing,
                },
                WindowSpec::Proportion { proportion: 0.1 },
            ],
        };

        let docs = parse_bed_files(&cfg).unwrap();
        let win_doc = docs
            .values()
            .find(|doc| doc.primary_type.starts_with("win"))
            .expect("window docs should be created from BED input");

        assert!(win_doc.primary_type.starts_with("win"));
        let has_window_feature_type = win_doc.attributes.as_ref().unwrap().iter().any(|attr| {
            attr.key == "feature_type"
                && matches!(
                    attr.keyword_value.as_ref(),
                    Some(crate::parse::genomehubs::StringOrVec::Multiple(values))
                        if values.iter().any(|v| v == "window")
                )
        });
        assert!(
            has_window_feature_type,
            "window primary type must also appear in feature_type metadata"
        );
    }

    #[test]
    fn test_parse_bed_files() {
        let bed_config_gc = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k1/GCA_016920705.1.GC.1k.bedGraph.gz"),
            local_path: Some(PathBuf::from("GCA_016920705.1.GC.1k.bedGraph.gz")),
            value_columns: vec![ValueColumn {
                label: "gc".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Mean,SummaryFunction::SubWindowVariance { size: 100 }],
            }],
        };
        let bed_config_n = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k1/GCA_016920705.1.N.1k.bedGraph.gz"),
            local_path: Some(PathBuf::from("GCA_016920705.1.N.1k.bedGraph.gz")),
            value_columns: vec![ValueColumn {
                label: "n".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Count, SummaryFunction::Mean, SummaryFunction::Sum],
            }],
        };
        let bed_config_at_skew = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k1/GCA_016920705.1.AT_skew.1k.bedGraph.gz"),
            local_path: Some(PathBuf::from("GCA_016920705.1.AT_skew.1k.bedGraph.gz")),
            value_columns: vec![ValueColumn {
                label: "at_skew".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Count, SummaryFunction::Mean, SummaryFunction::Sum],
            }],
        };
        let bed_config_gc_skew = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k1/GCA_016920705.1.GC_skew.1k.bedGraph.gz"),
            local_path: Some(PathBuf::from("GCA_016920705.1.GC_skew.1k.bedGraph.gz")),
            value_columns: vec![ValueColumn {
                label: "gc_skew".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Count, SummaryFunction::Mean, SummaryFunction::Sum],
            }],
        };
        let bed_config_shannon = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k1/GCA_016920705.1.nucShannon.1k.bedGraph.gz"),
            local_path: Some(PathBuf::from("GCA_016920705.1.nucShannon.1k.bedGraph.gz")),
            value_columns: vec![ValueColumn {
                label: "nucShannon".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Count, SummaryFunction::Mean, SummaryFunction::Sum],
            }],
        };
        let bed_config_cpg = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k2/GCA_016920705.1.CpG.1k.bedGraph.gz"),
            local_path: Some(PathBuf::from("GCA_016920705.1.CpG.1k.bedGraph.gz")),
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
            window_specs: vec![WindowSpec::Size {
                size: 1000000,
                remnant_policy: RemnantPolicy::Trailing,
            }],
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
