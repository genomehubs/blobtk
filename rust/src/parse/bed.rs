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
    pub(crate) fn compute(&self, data: &[f64]) -> f64 {
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalisationMethod {
    Zscore,
    RobustZscore,
    MinMax,
    #[serde(
        alias = "log2fc",
        alias = "log2_fc",
        alias = "log2_fold_change",
        alias = "l2fc"
    )]
    Log2FoldChange,
    #[serde(
        alias = "robust_log2_z",
        alias = "robust_log2_zscore",
        alias = "robust_log2_z_score",
        alias = "rlz",
        alias = "rlog2z"
    )]
    RobustLog2Zscore,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalisationStatistic {
    MeanStd,
    MedianMad,
    MinMax,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NormalisationScope {
    #[default]
    Assembly,
    Sequence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceAnchor {
    Midpoint,
    Start,
    End,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelomereState {
    Start,
    End,
    Both,
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NormalisationConfig {
    pub method: NormalisationMethod,
    pub statistic: NormalisationStatistic,
    #[serde(default)]
    pub scope: NormalisationScope,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValueColumn {
    pub label: String,
    pub index: usize,
    #[serde(rename = "type")]
    pub value_type: String,
    pub summary_functions: Vec<SummaryFunction>,
    #[serde(default)]
    pub normalisation: Option<NormalisationConfig>,
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

    pub fn transformed_name(&self) -> Option<String> {
        let normalisation = self.normalisation.as_ref()?;
        match normalisation.method {
            NormalisationMethod::Zscore => Some(format!("{}_zscore", self.label)),
            NormalisationMethod::RobustZscore => Some(format!("{}_rz", self.label)),
            NormalisationMethod::MinMax => Some(format!("{}_minmax", self.label)),
            NormalisationMethod::Log2FoldChange => Some(format!("{}_l2fc", self.label)),
            NormalisationMethod::RobustLog2Zscore => Some(format!("{}_rlz", self.label)),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct BedConfig {
    pub path: PathBuf,
    #[serde(default)]
    pub local_path: Option<PathBuf>,
    #[serde(default)]
    pub value_columns: Vec<ValueColumn>,
    #[serde(default)]
    pub has_header: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RemnantPolicy {
    Trailing,    // current behavior: leftover at chromosome end
    Centered,    // center the remnant by anchoring at both ends
    Symmetric,   // split the remnant evenly at both ends
    Discard,     // discard the remnant entirely
    Distributed, // distribute the remnant across the entire sequence
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RemnantBoundsConfig {
    pub min_fraction: f64,
    pub max_fraction: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WindowSpec {
    Size {
        size: usize,
        remnant_policy: RemnantPolicy,
        #[serde(default)]
        remnant_bounds: Option<RemnantBoundsConfig>,
    },
    Proportion {
        proportion: f64,
        #[serde(default)]
        min_size: Option<usize>,
    },
}

impl WindowSpec {
    pub fn to_string(&self) -> String {
        match self {
            WindowSpec::Size {
                size,
                remnant_policy: _,
                remnant_bounds: _,
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
            WindowSpec::Proportion {
                proportion,
                min_size: _,
            } => {
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
            remnant_bounds,
        } => {
            format!(
                "size:{}:{:?}:{:?}",
                size,
                remnant_policy,
                remnant_bounds
                    .as_ref()
                    .map(|bounds| (bounds.min_fraction, bounds.max_fraction))
            )
        }
        WindowSpec::Proportion {
            proportion,
            min_size,
        } => {
            format!("proportion:{}:{:?}", proportion, min_size)
        }
    }
}

/// A materialized array of bed-resolution step edges for a sequence. Every window
/// boundary is read directly from this array by step index, so a boundary can
/// never fall between steps: the last entry absorbs any trailing partial step.
struct BedSteps {
    /// (start_bp, end_bp) for each step, in order.
    edges: Vec<(usize, usize)>,
}

impl BedSteps {
    fn build(sequence_length: usize, step_size: usize) -> Self {
        if sequence_length == 0 || step_size == 0 {
            return BedSteps { edges: Vec::new() };
        }
        let full_steps = sequence_length / step_size;
        let remnant = sequence_length % step_size;
        let mut edges = Vec::with_capacity(full_steps + usize::from(remnant > 0));
        for i in 0..full_steps {
            edges.push((i * step_size, (i + 1) * step_size));
        }
        if remnant > 0 {
            edges.push((full_steps * step_size, sequence_length));
        }
        BedSteps { edges }
    }

    fn len(&self) -> usize {
        self.edges.len()
    }

    /// Convert a [start, end) range of step indices into (start_bp, end_bp).
    fn bp_range(&self, start_step: usize, end_step: usize) -> (usize, usize) {
        let start_bp = self.edges[start_step].0;
        let end_bp = self.edges[end_step - 1].1;
        (start_bp, end_bp)
    }
}

/// Partition `total_steps` into windows of `target_steps`, folding any leftover
/// steps symmetrically into the innermost windows around the midpoint so the
/// remnant never appears as its own undersized window at one end.
fn ends_inward_step_windows(
    total_steps: usize,
    target_steps: usize,
    min_fraction: f64,
    max_fraction: f64,
) -> Vec<(usize, usize)> {
    if total_steps == 0 {
        return Vec::new();
    }

    let target_steps = target_steps.max(1);
    let min_central = ((target_steps as f64 * min_fraction).round() as usize).max(1);
    let max_central = ((target_steps as f64 * max_fraction).round() as usize).max(min_central);

    let k_pairs = total_steps / (2 * target_steps);
    if k_pairs == 0 {
        if total_steps < min_central {
            return Vec::new();
        }
        if total_steps <= max_central {
            return vec![(0, total_steps)];
        }
        let mid = total_steps / 2;
        return vec![(0, mid), (mid, total_steps)];
    }

    let mut left_bounds: Vec<(usize, usize)> = Vec::with_capacity(k_pairs);
    let mut right_bounds: Vec<(usize, usize)> = Vec::with_capacity(k_pairs);
    for i in 0..k_pairs {
        let start = i * target_steps;
        left_bounds.push((start, start + target_steps));
        let right_start = total_steps - (i + 1) * target_steps;
        right_bounds.push((right_start, total_steps - i * target_steps));
    }
    right_bounds.reverse();

    let left_covered = k_pairs * target_steps;
    let right_covered_start = total_steps - k_pairs * target_steps;
    let middle_steps = right_covered_start - left_covered;

    let mut final_bounds = Vec::with_capacity(left_bounds.len() + right_bounds.len() + 2);
    if middle_steps == 0 {
        final_bounds.extend(left_bounds);
        final_bounds.extend(right_bounds);
    } else if middle_steps < min_central {
        let left_add = middle_steps / 2;
        let right_add = middle_steps - left_add;
        let mut left_bounds = left_bounds;
        let mut right_bounds = right_bounds;
        if let Some(last_left) = left_bounds.last_mut() {
            last_left.1 += left_add;
        }
        if let Some(first_right) = right_bounds.first_mut() {
            first_right.0 = first_right.0.saturating_sub(right_add);
        }
        final_bounds.extend(left_bounds);
        final_bounds.extend(right_bounds);
    } else if middle_steps <= max_central {
        final_bounds.extend(left_bounds);
        final_bounds.push((left_covered, right_covered_start));
        final_bounds.extend(right_bounds);
    } else {
        let mid_split = left_covered + middle_steps / 2;
        final_bounds.extend(left_bounds);
        final_bounds.push((left_covered, mid_split));
        final_bounds.push((mid_split, right_covered_start));
        final_bounds.extend(right_bounds);
    }

    final_bounds
}

/// Partition `total_steps` into `target_steps`-sized windows, merging the whole
/// leftover into a single central window (no envelope check).
fn centered_single_merge_step_windows(
    total_steps: usize,
    target_steps: usize,
) -> Vec<(usize, usize)> {
    if total_steps == 0 {
        return Vec::new();
    }
    let target_steps = target_steps.max(1);
    let full_bins = total_steps / target_steps;
    if full_bins == 0 {
        return vec![(0, total_steps)];
    }
    let remnant_steps = total_steps % target_steps;
    let central_index = full_bins / 2;

    let mut bounds = Vec::with_capacity(full_bins);
    let mut start_step = 0usize;
    for i in 0..full_bins {
        let fixed_end_step = (start_step + target_steps).min(total_steps);
        let end_step = if i == central_index {
            (fixed_end_step + remnant_steps).min(total_steps)
        } else {
            fixed_end_step
        };
        bounds.push((start_step, end_step));
        start_step = end_step;
    }
    bounds
}

/// Partition `total_steps` into `target_steps`-sized windows, splitting the
/// leftover between the two flanking windows nearest the midpoint.
fn symmetric_split_step_windows(total_steps: usize, target_steps: usize) -> Vec<(usize, usize)> {
    if total_steps == 0 {
        return Vec::new();
    }
    let target_steps = target_steps.max(1);
    let full_bins = total_steps / target_steps;
    if full_bins == 0 {
        return vec![(0, total_steps)];
    }
    let remnant_steps = total_steps % target_steps;
    let central_index = full_bins / 2;
    let left_extra = remnant_steps / 2;
    let right_extra = remnant_steps.saturating_sub(left_extra);

    let mut bounds = Vec::with_capacity(full_bins);
    let mut start_step = 0usize;
    for i in 0..full_bins {
        let fixed_end_step = (start_step + target_steps).min(total_steps);
        let end_step = if i == central_index {
            if full_bins > 1 {
                let left_bound = (fixed_end_step + left_extra).min(total_steps);
                (total_steps - right_extra).max(left_bound)
            } else {
                (fixed_end_step + left_extra).min(total_steps)
            }
        } else {
            fixed_end_step
        };
        bounds.push((start_step, end_step));
        start_step = end_step;
    }
    if start_step < total_steps {
        bounds.push((start_step, total_steps));
    }
    bounds
}

pub fn window_bounds_for_sequence(
    sequence_length: usize,
    window_spec: &WindowSpec,
    lines_per_unit: usize,
) -> Vec<(usize, usize)> {
    if sequence_length == 0 {
        return Vec::new();
    }
    let step_size = lines_per_unit.max(1);
    let steps = BedSteps::build(sequence_length, step_size);
    let total_steps = steps.len();
    if total_steps == 0 {
        return Vec::new();
    }

    let step_ranges: Vec<(usize, usize)> = match window_spec {
        WindowSpec::Size {
            size,
            remnant_policy,
            remnant_bounds,
        } => {
            if *size == 0 {
                return Vec::new();
            }
            let target_steps = ((size + step_size - 1) / step_size).max(1);

            match remnant_policy {
                RemnantPolicy::Trailing => {
                    let mut ranges = Vec::new();
                    let mut start_step = 0usize;
                    while start_step < total_steps {
                        let end_step = (start_step + target_steps).min(total_steps);
                        ranges.push((start_step, end_step));
                        start_step = end_step;
                    }
                    ranges
                }
                RemnantPolicy::Discard => {
                    let mut ranges = Vec::new();
                    let mut start_step = 0usize;
                    while start_step + target_steps <= total_steps {
                        ranges.push((start_step, start_step + target_steps));
                        start_step += target_steps;
                    }
                    ranges
                }
                RemnantPolicy::Distributed => {
                    let num_windows =
                        ((total_steps as f64 / target_steps as f64).ceil() as usize).max(1);
                    let mut ranges = Vec::with_capacity(num_windows);
                    let mut start_step = 0usize;
                    for i in 0..num_windows {
                        let end_step = if i + 1 == num_windows {
                            total_steps
                        } else {
                            (((i + 1) * target_steps).min(total_steps)).max(start_step)
                        };
                        ranges.push((start_step, end_step));
                        start_step = end_step;
                    }
                    ranges
                }
                RemnantPolicy::Centered => match remnant_bounds {
                    Some(bounds_cfg) => ends_inward_step_windows(
                        total_steps,
                        target_steps,
                        bounds_cfg.min_fraction,
                        bounds_cfg.max_fraction,
                    ),
                    None => centered_single_merge_step_windows(total_steps, target_steps),
                },
                RemnantPolicy::Symmetric => symmetric_split_step_windows(total_steps, target_steps),
            }
        }
        WindowSpec::Proportion {
            proportion,
            min_size,
        } => {
            if *proportion <= 0.0 {
                return Vec::new();
            }
            // A short sequence can't hold a full complement of proportion windows at
            // min_size, so skip entirely rather than emit a single oversized window.
            if let Some(min_size) = min_size {
                if *min_size > 0 && (sequence_length as f64) < (*min_size as f64) / proportion {
                    return Vec::new();
                }
            }
            let minimum_step_size = min_size
                .map(|min_size| ((min_size + step_size - 1) / step_size).max(1))
                .unwrap_or(1);
            let target_step_size = (((sequence_length as f64 * proportion) / step_size as f64)
                .ceil() as usize)
                .max(minimum_step_size)
                .max(1);
            let num_windows = if target_step_size >= total_steps {
                1
            } else {
                ((total_steps as f64 / target_step_size as f64).ceil() as usize).max(1)
            };

            let mut ranges = Vec::with_capacity(num_windows);
            let mut start_step = 0usize;
            for idx in 0..num_windows {
                let end_step = if idx + 1 == num_windows {
                    total_steps
                } else {
                    (((idx + 1) * target_step_size).min(total_steps)).max(start_step)
                };
                ranges.push((start_step, end_step));
                start_step = end_step;
            }
            ranges
        }
    };

    step_ranges
        .into_iter()
        .map(|(start_step, end_step)| steps.bp_range(start_step, end_step))
        .collect()
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MultiBedConfig {
    pub accession: String,
    #[serde(default)]
    pub taxon_id: String,
    #[serde(default)]
    pub ancestors: Vec<String>,
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
    if bed_config.has_header {
        // Skip the header line
        let mut header = String::new();
        bed_reader.read_line(&mut header).map_err(|e| {
            error::Error::ReaderError(format!(
                "Error reading header line from BED file {}: {}",
                bed_path.display(),
                e
            ))
        })?;
    }
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

fn mean_std_stats(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }

    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = if values.len() <= 1 {
        0.0
    } else {
        let diff = values
            .iter()
            .map(|value| {
                let delta = value - mean;
                delta * delta
            })
            .sum::<f64>();
        diff / (values.len() - 1) as f64
    };
    let std_dev = variance.sqrt();
    (mean, std_dev)
}

fn median_and_mad(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    let median = if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    };

    let mut deviations: Vec<f64> = sorted.iter().map(|value| (value - median).abs()).collect();
    deviations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mad_mid = deviations.len() / 2;
    let mad = if deviations.is_empty() {
        0.0
    } else if deviations.len() % 2 == 0 {
        (deviations[mad_mid - 1] + deviations[mad_mid]) / 2.0
    } else {
        deviations[mad_mid]
    };

    (median, mad)
}

fn min_max_stats(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }

    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (min, max)
}

fn transform_value_for_normalisation(value: f64, config: &NormalisationConfig) -> f64 {
    match config.method {
        NormalisationMethod::Log2FoldChange | NormalisationMethod::RobustLog2Zscore => {
            if value <= -1.0 {
                0.0
            } else {
                (value + 1.0).log2()
            }
        }
        _ => value,
    }
}

pub(crate) fn normalisation_stats_for_config(
    values: &[f64],
    config: &NormalisationConfig,
) -> (f64, f64) {
    let transformed_values: Vec<f64> = values
        .iter()
        .map(|value| transform_value_for_normalisation(*value, config))
        .collect();

    match config.statistic {
        NormalisationStatistic::MeanStd => mean_std_stats(&transformed_values),
        NormalisationStatistic::MedianMad => median_and_mad(&transformed_values),
        NormalisationStatistic::MinMax => min_max_stats(&transformed_values),
    }
}

pub(crate) fn apply_normalisation(
    value: f64,
    config: &NormalisationConfig,
    from: (f64, f64),
) -> f64 {
    let transformed_value = transform_value_for_normalisation(value, config);

    match config.method {
        NormalisationMethod::Zscore => {
            if from.1.is_nan() || from.1 == 0.0 {
                0.0
            } else {
                (transformed_value - from.0) / from.1
            }
        }
        NormalisationMethod::RobustZscore => {
            if from.1.is_nan() || from.1 == 0.0 {
                0.0
            } else {
                (transformed_value - from.0) / from.1
            }
        }
        NormalisationMethod::MinMax => {
            let (min, max) = from;
            if max.is_nan() || min.is_nan() || max <= min {
                0.0
            } else {
                (transformed_value - min) / (max - min)
            }
        }
        NormalisationMethod::Log2FoldChange => {
            if from.0.is_nan() {
                0.0
            } else {
                transformed_value - from.0
            }
        }
        NormalisationMethod::RobustLog2Zscore => {
            if from.1.is_nan() || from.1 == 0.0 {
                0.0
            } else {
                (transformed_value - from.0) / from.1
            }
        }
    }
}

pub fn distance_to_telomere(
    sequence_length: usize,
    start: usize,
    end: usize,
    anchor: DistanceAnchor,
) -> f64 {
    if sequence_length == 0 {
        return 0.0;
    }

    let position = match anchor {
        DistanceAnchor::Midpoint => (start + end) as f64 / 2.0,
        DistanceAnchor::Start => start as f64,
        DistanceAnchor::End => end as f64,
    };

    let start_distance = position;
    let end_distance = (sequence_length as f64 - position).abs();
    start_distance.min(end_distance)
}

pub fn telomere_state_for_sequence(
    sequence_length: usize,
    has_start_telomere: bool,
    has_end_telomere: bool,
) -> TelomereState {
    if sequence_length == 0 {
        return TelomereState::None;
    }

    match (has_start_telomere, has_end_telomere) {
        (true, true) => TelomereState::Both,
        (true, false) => TelomereState::Start,
        (false, true) => TelomereState::End,
        (false, false) => TelomereState::None,
    }
}

pub fn nearest_telomere_valid(
    sequence_length: usize,
    start: usize,
    end: usize,
    anchor: DistanceAnchor,
    has_start_telomere: bool,
    has_end_telomere: bool,
) -> bool {
    if !has_start_telomere && !has_end_telomere {
        return false;
    }

    let position = match anchor {
        DistanceAnchor::Midpoint => (start + end) as f64 / 2.0,
        DistanceAnchor::Start => start as f64,
        DistanceAnchor::End => end as f64,
    };

    let start_distance = position;
    let end_distance = (sequence_length as f64 - position).abs();
    let nearest_is_start = start_distance <= end_distance;

    match (has_start_telomere, has_end_telomere) {
        (true, true) => true,
        (true, false) => nearest_is_start,
        (false, true) => !nearest_is_start,
        (false, false) => false,
    }
}

pub fn window_flags_for_telomere(
    sequence_length: usize,
    start: usize,
    end: usize,
    anchor: DistanceAnchor,
    has_start_telomere: bool,
    has_end_telomere: bool,
    has_internal_telomeres: bool,
) -> Vec<String> {
    let mut flags = Vec::new();
    if nearest_telomere_valid(
        sequence_length,
        start,
        end,
        anchor,
        has_start_telomere,
        has_end_telomere,
    ) {
        flags.push("nearest_telomere_valid".to_string());
    }
    if has_internal_telomeres {
        flags.push("has_internal_telomeres".to_string());
    }
    flags
}

pub fn parse_bed_files(
    config: &MultiBedConfig,
    canonical_sequence_lengths: Option<&HashMap<String, usize>>,
) -> Result<HashMap<String, FeatureDocument>, error::Error> {
    let mut feature_docs: HashMap<String, FeatureDocument> = HashMap::new();

    for bed_config in &config.bed_configs {
        if let Ok(per_seq_buffers) = read_bed_file(bed_config) {
            let assembly_stats: Vec<(f64, f64)> = bed_config
                .value_columns
                .iter()
                .enumerate()
                .map(|(index, column)| {
                    let values: Vec<f64> = per_seq_buffers
                        .values()
                        .flat_map(|buffer| buffer.iter())
                        .flat_map(|feature| feature.values.get(index).copied())
                        .collect();
                    column
                        .normalisation
                        .as_ref()
                        .map_or((0.0, 0.0), |normalisation| {
                            normalisation_stats_for_config(&values, normalisation)
                        })
                })
                .collect();

            let sequence_stats: HashMap<String, Vec<(f64, f64)>> = per_seq_buffers
                .iter()
                .map(|(seq_id, buffer)| {
                    let stats = bed_config
                        .value_columns
                        .iter()
                        .enumerate()
                        .map(|(index, column)| {
                            let values: Vec<f64> = buffer
                                .iter()
                                .flat_map(|feature| feature.values.get(index).copied())
                                .collect();
                            column
                                .normalisation
                                .as_ref()
                                .map_or((0.0, 0.0), |normalisation| {
                                    normalisation_stats_for_config(&values, normalisation)
                                })
                        })
                        .collect();
                    (seq_id.clone(), stats)
                })
                .collect();

            for (seq_id, buffer) in per_seq_buffers {
                // Use the canonical (sequence-report) length when available so every
                // bed track and the BUSCO/synteny window assignment agree on the same
                // window boundaries; otherwise fall back to this track's own extent.
                let bed_derived_length =
                    buffer.iter().map(|feature| feature.end).max().unwrap_or(0);
                let sequence_length = canonical_sequence_lengths
                    .and_then(|lengths| lengths.get(&seq_id).copied())
                    .unwrap_or(bed_derived_length);
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
                                Some(config.ancestors.clone()),
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

                            if let Some(normalisation) =
                                &bed_config.value_columns[index].normalisation
                            {
                                let primary_summary = bed_config.value_columns[index]
                                    .summary_functions
                                    .first()
                                    .cloned()
                                    .unwrap_or(SummaryFunction::Mean);
                                let summary_value =
                                    summary.get(&primary_summary).copied().unwrap_or(f64::NAN);
                                if !summary_value.is_nan() {
                                    let transformed_key = bed_config.value_columns[index]
                                        .transformed_name()
                                        .unwrap_or_else(|| {
                                            format!(
                                                "{}_zscore",
                                                bed_config.value_columns[index].label
                                            )
                                        });
                                    let scope_stats = match normalisation.scope {
                                        NormalisationScope::Assembly => {
                                            assembly_stats.get(index).copied().unwrap_or((0.0, 0.0))
                                        }
                                        NormalisationScope::Sequence => sequence_stats
                                            .get(&seq_id)
                                            .and_then(|stats| stats.get(index).copied())
                                            .unwrap_or((0.0, 0.0)),
                                    };
                                    let transformed_value = apply_normalisation(
                                        summary_value,
                                        normalisation,
                                        scope_stats,
                                    );
                                    if doc.attributes.is_none() {
                                        doc.attributes = Some(vec![]);
                                    }
                                    doc.attributes.as_mut().unwrap().push(NestedAttribute {
                                        key: transformed_key,
                                        half_float_value: Some(transformed_value as f32),
                                        ..Default::default()
                                    });
                                }
                            }
                        }

                        let distance_to_telomere_value = distance_to_telomere(
                            sequence_length,
                            window_start,
                            window_end,
                            DistanceAnchor::Midpoint,
                        );
                        let window_flags = window_flags_for_telomere(
                            sequence_length,
                            window_start,
                            window_end,
                            DistanceAnchor::Midpoint,
                            window_start == 0,
                            window_end >= sequence_length,
                            false,
                        );

                        let attributes = doc.attributes.as_mut().unwrap();
                        attributes.push(NestedAttribute {
                            key: "distance_to_telomere".to_string(),
                            double_value: Some(distance_to_telomere_value as f64),
                            ..Default::default()
                        });
                        if !window_flags.is_empty() {
                            attributes.push(NestedAttribute {
                                key: "window_flags".to_string(),
                                keyword_value: Some(super::genomehubs::StringOrVec::Multiple(
                                    window_flags,
                                )),
                                ..Default::default()
                            });
                        }
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
                normalisation: None,
            },
            ValueColumn {
                label: "value2".to_string(),
                index: 4,
                value_type: "int".to_string(),
                summary_functions: vec![SummaryFunction::Sum],
                normalisation: None,
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
    fn test_assembly_local_zscore_normalisation() {
        let config = NormalisationConfig {
            method: NormalisationMethod::Zscore,
            statistic: NormalisationStatistic::MeanStd,
            scope: NormalisationScope::Assembly,
        };

        let value = 0.8;
        let stats = (0.5, 0.2);
        let transformed = apply_normalisation(value, &config, stats);

        assert!((transformed - 1.5).abs() < 1e-9);
    }

    #[test]
    fn test_assembly_local_robust_zscore_normalisation() {
        let config = NormalisationConfig {
            method: NormalisationMethod::RobustZscore,
            statistic: NormalisationStatistic::MedianMad,
            scope: NormalisationScope::Assembly,
        };

        let value = 5.0;
        let stats = median_and_mad(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let transformed = apply_normalisation(value, &config, stats);

        assert!((transformed - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_assembly_local_minmax_normalisation() {
        let config = NormalisationConfig {
            method: NormalisationMethod::MinMax,
            statistic: NormalisationStatistic::MinMax,
            scope: NormalisationScope::Assembly,
        };

        let value = 0.4;
        let stats = (0.0, 1.0);
        let transformed = apply_normalisation(value, &config, stats);

        assert!((transformed - 0.4).abs() < 1e-9);
    }

    #[test]
    fn test_distance_to_telomere_uses_nearest_boundary() {
        assert!((distance_to_telomere(100, 40, 60, DistanceAnchor::Midpoint) - 50.0).abs() < 1e-9);
        assert!((distance_to_telomere(100, 0, 10, DistanceAnchor::Midpoint) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_window_flags_for_telomere_state() {
        let flags =
            window_flags_for_telomere(100, 0, 10, DistanceAnchor::Midpoint, true, false, false);
        assert_eq!(flags, vec!["nearest_telomere_valid".to_string()]);

        let flags =
            window_flags_for_telomere(100, 40, 60, DistanceAnchor::Midpoint, true, true, true);
        assert!(flags.contains(&"nearest_telomere_valid".to_string()));
        assert!(flags.contains(&"has_internal_telomeres".to_string()));
    }

    #[test]
    fn test_nearest_telomere_valid_for_single_telomere_chromosome() {
        assert!(nearest_telomere_valid(
            100,
            0,
            10,
            DistanceAnchor::Midpoint,
            true,
            false,
        ));
        assert!(!nearest_telomere_valid(
            100,
            80,
            90,
            DistanceAnchor::Midpoint,
            true,
            false,
        ));
        assert!(nearest_telomere_valid(
            100,
            90,
            100,
            DistanceAnchor::Midpoint,
            false,
            true,
        ));
    }

    #[test]
    fn test_assembly_local_log2_fold_change_normalisation() {
        let config = NormalisationConfig {
            method: NormalisationMethod::Log2FoldChange,
            statistic: NormalisationStatistic::MeanStd,
            scope: NormalisationScope::Assembly,
        };

        let value = 7.0;
        let stats = (2.0, 0.0);
        let transformed = apply_normalisation(value, &config, stats);

        assert!((transformed - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_assembly_local_robust_log2_zscore_normalisation() {
        let config = NormalisationConfig {
            method: NormalisationMethod::RobustLog2Zscore,
            statistic: NormalisationStatistic::MedianMad,
            scope: NormalisationScope::Assembly,
        };

        let value = 7.0;
        let stats = (2.0, 1.0);
        let transformed = apply_normalisation(value, &config, stats);

        assert!((transformed - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_window_bounds_for_sequence_symmetric_remnant() {
        let bounds = window_bounds_for_sequence(
            10,
            &WindowSpec::Size {
                size: 4,
                remnant_policy: RemnantPolicy::Symmetric,
                remnant_bounds: None,
            },
            1,
        );
        assert_eq!(bounds, vec![(0, 4), (4, 9), (9, 10)]);
    }

    #[test]
    fn test_window_bounds_for_sequence_centered_remnant() {
        let bounds = window_bounds_for_sequence(
            14,
            &WindowSpec::Size {
                size: 4,
                remnant_policy: RemnantPolicy::Centered,
                remnant_bounds: None,
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
                remnant_bounds: None,
            },
            1,
            &mut cache,
        );
        assert_eq!(window_ids, vec!["chr1:4-10:win-4".to_string()]);
    }

    #[test]
    fn test_window_bounds_for_sequence_respects_remnant_bounds() {
        let bounds = window_bounds_for_sequence(
            10_000,
            &WindowSpec::Size {
                size: 4_000,
                remnant_policy: RemnantPolicy::Centered,
                remnant_bounds: Some(RemnantBoundsConfig {
                    min_fraction: 0.67,
                    max_fraction: 1.33,
                }),
            },
            1,
        );

        assert_eq!(bounds, vec![(0, 5_000), (5_000, 10_000)]);
    }

    #[test]
    fn test_window_bounds_for_sequence_centered_remnant_stays_balanced_when_bin_plus_remnant() {
        let bounds = window_bounds_for_sequence(
            1_800_000,
            &WindowSpec::Size {
                size: 1_000_000,
                remnant_policy: RemnantPolicy::Centered,
                remnant_bounds: Some(RemnantBoundsConfig {
                    min_fraction: 0.67,
                    max_fraction: 1.33,
                }),
            },
            1,
        );

        assert_eq!(bounds, vec![(0, 900_000), (900_000, 1_800_000)]);
    }

    #[test]
    fn test_window_bounds_for_sequence_skips_short_scaffolds_below_min_remnant_band() {
        let bounds = window_bounds_for_sequence(
            1_000,
            &WindowSpec::Size {
                size: 1_000_000,
                remnant_policy: RemnantPolicy::Centered,
                remnant_bounds: Some(RemnantBoundsConfig {
                    min_fraction: 0.67,
                    max_fraction: 1.33,
                }),
            },
            1,
        );

        assert!(bounds.is_empty());
    }

    #[test]
    fn test_window_bounds_for_sequence_splits_overlong_centered_remnant_around_midpoint() {
        let bounds = window_bounds_for_sequence(
            3_800_000,
            &WindowSpec::Size {
                size: 1_000_000,
                remnant_policy: RemnantPolicy::Centered,
                remnant_bounds: Some(RemnantBoundsConfig {
                    min_fraction: 0.67,
                    max_fraction: 1.33,
                }),
            },
            1,
        );

        assert_eq!(
            bounds,
            vec![
                (0, 1_000_000),
                (1_000_000, 1_900_000),
                (1_900_000, 2_800_000),
                (2_800_000, 3_800_000),
            ]
        );
    }

    #[test]
    fn test_window_bounds_for_sequence_proportion_skips_sequence_shorter_than_min_size_over_proportion(
    ) {
        // A 15kb mitochondrion can't hold even one min_size window at 10% proportion.
        let bounds = window_bounds_for_sequence(
            15_000,
            &WindowSpec::Proportion {
                proportion: 0.1,
                min_size: Some(100_000),
            },
            1_000,
        );

        assert!(bounds.is_empty());
    }

    #[test]
    fn test_window_bounds_for_sequence_size_remnant_respects_bed_resolution() {
        let bounds = window_bounds_for_sequence(
            8_500,
            &WindowSpec::Size {
                size: 3_000,
                remnant_policy: RemnantPolicy::Centered,
                remnant_bounds: None,
            },
            1_000,
        );

        assert_eq!(bounds, vec![(0, 3_000), (3_000, 6_000), (6_000, 8_500)]);
    }

    #[test]
    fn test_window_bounds_for_sequence_respects_proportion_min_size() {
        let bounds = window_bounds_for_sequence(
            20_000,
            &WindowSpec::Proportion {
                proportion: 0.25,
                min_size: Some(4_000),
            },
            1_000,
        );

        assert_eq!(
            bounds,
            vec![
                (0, 5_000),
                (5_000, 10_000),
                (10_000, 15_000),
                (15_000, 20_000)
            ]
        );
    }

    #[test]
    fn test_window_bounds_all_interior_boundaries_are_step_aligned() {
        let step = 1_000usize;
        let specs = vec![
            WindowSpec::Size {
                size: 1_000_000,
                remnant_policy: RemnantPolicy::Centered,
                remnant_bounds: Some(RemnantBoundsConfig {
                    min_fraction: 0.67,
                    max_fraction: 1.33,
                }),
            },
            WindowSpec::Size {
                size: 1_000_000,
                remnant_policy: RemnantPolicy::Symmetric,
                remnant_bounds: None,
            },
            WindowSpec::Proportion {
                proportion: 0.1,
                min_size: Some(100_000),
            },
        ];

        for sequence_length in [8_500usize, 1_234_567, 3_800_321, 950_003] {
            for spec in &specs {
                let bounds = window_bounds_for_sequence(sequence_length, spec, step);
                for &(start, end) in &bounds {
                    assert_eq!(
                        start % step,
                        0,
                        "start {start} not step-aligned for {spec:?} over {sequence_length}"
                    );
                    assert!(
                        end % step == 0 || end == sequence_length,
                        "end {end} not step-aligned for {spec:?} over {sequence_length}"
                    );
                }
            }
        }
    }

    // temporary test for parse_bed_files function
    #[test]
    fn test_parse_bed_files_uses_max_end_for_sequence_length() {
        let tmp = std::env::temp_dir().join("blobtk_bed_sequence_length_regression.bed");
        std::fs::write(
            &tmp,
            "chr1\t7000\t8000\t0.1\nchr1\t0\t5000\t0.2\nchr1\t5000\t7000\t0.3\nchr2\t0\t12000\t0.4\n",
        )
        .unwrap();

        let cfg = MultiBedConfig {
            accession: "GCA_test".to_string(),
            taxon_id: "123".to_string(),
            ancestors: vec!["1".to_string(), "2".to_string()],
            lines_per_unit: 1000,
            bed_configs: vec![BedConfig {
                path: tmp,
                local_path: None,
                value_columns: vec![ValueColumn {
                    label: "gc".to_string(),
                    index: 3,
                    value_type: "float".to_string(),
                    summary_functions: vec![SummaryFunction::Mean],
                    normalisation: None,
                }],
                has_header: false,
            }],
            window_specs: vec![WindowSpec::Size {
                size: 5000,
                remnant_policy: RemnantPolicy::Trailing,
                remnant_bounds: None,
            }],
        };

        let docs = parse_bed_files(&cfg, None).unwrap();
        assert!(docs.keys().any(|id| id.contains("chr1:0-5000:win-5k")));
        assert!(docs.keys().any(|id| id.contains("chr1:5000-8000:win-5k")));
        assert!(!docs.keys().any(|id| id.contains("chr1:7000-8000:win-5k")));
    }

    #[test]
    fn test_parse_bed_files_respects_sequence_scoped_normalisation() {
        let tmp = std::env::temp_dir().join("blobtk_bed_sequence_scope_regression.bed");
        std::fs::write(
            &tmp,
            "chr1\t0\t1000\t0.2\nchr1\t1000\t2000\t0.8\nchr1\t2000\t3000\t0.6\n",
        )
        .unwrap();

        let cfg = MultiBedConfig {
            accession: "GCA_test".to_string(),
            taxon_id: "123".to_string(),
            ancestors: vec!["1".to_string(), "2".to_string()],
            lines_per_unit: 1000,
            bed_configs: vec![BedConfig {
                path: tmp,
                local_path: None,
                value_columns: vec![ValueColumn {
                    label: "gc".to_string(),
                    index: 3,
                    value_type: "float".to_string(),
                    summary_functions: vec![SummaryFunction::Mean],
                    normalisation: Some(NormalisationConfig {
                        method: NormalisationMethod::Zscore,
                        statistic: NormalisationStatistic::MeanStd,
                        scope: NormalisationScope::Sequence,
                    }),
                }],
                has_header: false,
            }],
            window_specs: vec![WindowSpec::Size {
                size: 3000,
                remnant_policy: RemnantPolicy::Trailing,
                remnant_bounds: None,
            }],
        };

        let docs = parse_bed_files(&cfg, None).unwrap();
        let window = docs
            .values()
            .find(|doc| doc.sequence_id == "chr1" && doc.primary_type.starts_with("win"))
            .expect("sequence-scoped normalisation should create a window for chr1");
        let attr = window
            .attributes
            .as_ref()
            .expect("window should retain transformed attribute")
            .iter()
            .find(|attr| attr.key == "gc_zscore")
            .expect("sequence-scoped z-score should be attached");
        assert!(!attr.half_float_value.unwrap().is_nan());
    }

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
            ancestors: vec!["1".to_string(), "2".to_string()],
            lines_per_unit: 1000,
            bed_configs: vec![BedConfig {
                path: tmp,
                local_path: None,
                value_columns: vec![ValueColumn {
                    label: "gc".to_string(),
                    index: 3,
                    value_type: "float".to_string(),
                    summary_functions: vec![SummaryFunction::Mean],
                    normalisation: None,
                }],
                has_header: false,
            }],
            window_specs: vec![WindowSpec::Size {
                size: 2000,
                remnant_policy: RemnantPolicy::Trailing,
                remnant_bounds: None,
            }],
        };

        let docs = parse_bed_files(&cfg, None).unwrap();
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
            ancestors: vec!["1".to_string(), "2".to_string()],
            lines_per_unit: 1000,
            bed_configs: vec![BedConfig {
                path: tmp,
                local_path: None,
                value_columns: vec![ValueColumn {
                    label: "gc".to_string(),
                    index: 3,
                    value_type: "float".to_string(),
                    summary_functions: vec![SummaryFunction::Mean],
                    normalisation: None,
                }],
                has_header: false,
            }],
            window_specs: vec![WindowSpec::Size {
                size: 2000,
                remnant_policy: RemnantPolicy::Centered,
                remnant_bounds: None,
            }],
        };

        let docs = parse_bed_files(&cfg, None).unwrap();
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
            ancestors: vec!["1".to_string(), "2".to_string()],
            lines_per_unit: 1000,
            bed_configs: vec![BedConfig {
                path: tmp,
                local_path: None,
                value_columns: vec![ValueColumn {
                    label: "gc".to_string(),
                    index: 3,
                    value_type: "float".to_string(),
                    summary_functions: vec![SummaryFunction::Mean],
                    normalisation: None,
                }],
                has_header: false,
            }],
            window_specs: vec![
                WindowSpec::Size {
                    size: 2000,
                    remnant_policy: RemnantPolicy::Trailing,
                    remnant_bounds: None,
                },
                WindowSpec::Proportion {
                    proportion: 0.1,
                    min_size: None,
                },
            ],
        };

        let docs = parse_bed_files(&cfg, None).unwrap();
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
    fn test_parse_bed_files_uses_double_for_distance_to_telomere() {
        let tmp = std::env::temp_dir().join("blobtk_bed_distance_to_telomere_double.bed");
        std::fs::write(&tmp, "chr1\t0\t1000\t0.1\nchr1\t1000\t2000\t0.2\n").unwrap();

        let cfg = MultiBedConfig {
            accession: "GCA_test".to_string(),
            taxon_id: "123".to_string(),
            ancestors: vec!["1".to_string(), "2".to_string()],
            lines_per_unit: 1000,
            bed_configs: vec![BedConfig {
                path: tmp,
                local_path: None,
                value_columns: vec![ValueColumn {
                    label: "gc".to_string(),
                    index: 3,
                    value_type: "float".to_string(),
                    summary_functions: vec![SummaryFunction::Mean],
                    normalisation: None,
                }],
                has_header: false,
            }],
            window_specs: vec![WindowSpec::Size {
                size: 2000,
                remnant_policy: RemnantPolicy::Trailing,
                remnant_bounds: None,
            }],
        };

        let docs = parse_bed_files(&cfg, None).unwrap();
        let window = docs
            .values()
            .find(|doc| doc.primary_type.starts_with("win"))
            .expect("window docs should be created from BED input");

        let has_double_distance = window
            .attributes
            .as_ref()
            .unwrap()
            .iter()
            .any(|attr| attr.key == "distance_to_telomere" && attr.double_value.is_some());
        assert!(
            has_double_distance,
            "distance_to_telomere must be stored as a double to match the registry and histogram contracts"
        );
    }

    #[test]
    fn test_parse_bed_files_proportion_windows_are_consistent_across_tracks_with_canonical_length()
    {
        // Two tracks whose own data happens to stop at different bp offsets
        // (e.g. GC vs N content bedgraphs with different trailing coverage).
        let gc_tmp = std::env::temp_dir().join("blobtk_bed_gc_track_length_mismatch.bed");
        std::fs::write(
            &gc_tmp,
            "chr1\t0\t1000\t0.1\nchr1\t1000\t9000\t0.2\nchr1\t9000\t10000\t0.3\n",
        )
        .unwrap();
        let n_tmp = std::env::temp_dir().join("blobtk_bed_n_track_length_mismatch.bed");
        std::fs::write(&n_tmp, "chr1\t0\t1000\t0.0\nchr1\t1000\t9500\t0.0\n").unwrap();

        let cfg = MultiBedConfig {
            accession: "GCA_test".to_string(),
            taxon_id: "123".to_string(),
            ancestors: vec!["1".to_string(), "2".to_string()],
            lines_per_unit: 1000,
            bed_configs: vec![
                BedConfig {
                    path: gc_tmp,
                    local_path: None,
                    value_columns: vec![ValueColumn {
                        label: "gc".to_string(),
                        index: 3,
                        value_type: "float".to_string(),
                        summary_functions: vec![SummaryFunction::Mean],
                        normalisation: None,
                    }],
                    has_header: false,
                },
                BedConfig {
                    path: n_tmp,
                    local_path: None,
                    value_columns: vec![ValueColumn {
                        label: "n".to_string(),
                        index: 3,
                        value_type: "float".to_string(),
                        summary_functions: vec![SummaryFunction::Mean],
                        normalisation: None,
                    }],
                    has_header: false,
                },
            ],
            window_specs: vec![WindowSpec::Proportion {
                proportion: 0.5,
                min_size: None,
            }],
        };

        // Without a canonical length, the two tracks disagree (10_000 vs 9_500)
        // and produce different window boundaries for the same nominal windows.
        let docs_without_canonical = parse_bed_files(&cfg, None).unwrap();
        let mut window_names: Vec<_> = docs_without_canonical
            .values()
            .filter(|doc| doc.primary_type.starts_with("win"))
            .map(|doc| doc.feature_id.clone())
            .collect();
        window_names.sort();
        assert!(
            window_names.len() > 2,
            "tracks with mismatched lengths fragment into more than the expected 2 windows: {window_names:?}"
        );

        // With a canonical length shared by both tracks, both produce the same
        // two window boundaries and their attributes land on the same documents.
        let canonical_lengths = HashMap::from([("chr1".to_string(), 10_000usize)]);
        let docs_with_canonical = parse_bed_files(&cfg, Some(&canonical_lengths)).unwrap();
        let mut window_names: Vec<_> = docs_with_canonical
            .values()
            .filter(|doc| doc.primary_type.starts_with("win"))
            .map(|doc| doc.feature_id.clone())
            .collect();
        window_names.sort();
        assert_eq!(
            window_names,
            vec![
                "chr1:0-5000:win-0.50".to_string(),
                "chr1:5000-10000:win-0.50".to_string(),
            ]
        );
        for doc in docs_with_canonical
            .values()
            .filter(|doc| doc.primary_type.starts_with("win"))
        {
            let keys: Vec<_> = doc
                .attributes
                .as_ref()
                .unwrap()
                .iter()
                .map(|attr| attr.key.as_str())
                .collect();
            assert!(
                keys.contains(&"gc") && keys.contains(&"n"),
                "both tracks should contribute to the same window doc: {keys:?}"
            );
        }
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
                normalisation: None,
            }],
            has_header: false
        };
        let bed_config_n = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k1/GCA_016920705.1.N.1k.bedGraph.gz"),
            local_path: Some(PathBuf::from("GCA_016920705.1.N.1k.bedGraph.gz")),
            value_columns: vec![ValueColumn {
                label: "n".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Count, SummaryFunction::Mean, SummaryFunction::Sum],
                normalisation: None,
            }],
            has_header: false,
        };
        let bed_config_at_skew = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k1/GCA_016920705.1.AT_skew.1k.bedGraph.gz"),
            local_path: Some(PathBuf::from("GCA_016920705.1.AT_skew.1k.bedGraph.gz")),
            value_columns: vec![ValueColumn {
                label: "at_skew".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Count, SummaryFunction::Mean, SummaryFunction::Sum],
                normalisation: None,
            }],
            has_header: false,
        };
        let bed_config_gc_skew = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k1/GCA_016920705.1.GC_skew.1k.bedGraph.gz"),
            local_path: Some(PathBuf::from("GCA_016920705.1.GC_skew.1k.bedGraph.gz")),
            value_columns: vec![ValueColumn {
                label: "gc_skew".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Count, SummaryFunction::Mean, SummaryFunction::Sum],
                normalisation: None,
            }],
            has_header: false,
        };
        let bed_config_shannon = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k1/GCA_016920705.1.nucShannon.1k.bedGraph.gz"),
            local_path: Some(PathBuf::from("GCA_016920705.1.nucShannon.1k.bedGraph.gz")),
            value_columns: vec![ValueColumn {
                label: "nucShannon".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Count, SummaryFunction::Mean, SummaryFunction::Sum],
                normalisation: None,
            }],
            has_header: false,
        };
        let bed_config_cpg = BedConfig {
            path: PathBuf::from("https://gap.cog.sanger.ac.uk/GCA_016920705.1/base_content/k2/GCA_016920705.1.CpG.1k.bedGraph.gz"),
            local_path: Some(PathBuf::from("GCA_016920705.1.CpG.1k.bedGraph.gz")),
            value_columns: vec![ValueColumn {
                label: "cpg".to_string(),
                index: 3,
                value_type: "float".to_string(),
                summary_functions: vec![SummaryFunction::Mean, SummaryFunction::SubWindowVariance { size: 100 }],
                normalisation: None,
            }],
            has_header: false,
        };

        let multi_bed_config = MultiBedConfig {
            accession: "GCA_016920705.1".to_string(),
            taxon_id: "1518534".to_string(),
            ancestors: vec!["44542".to_string(), "44537".to_string(), "7164".to_string()],
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
                remnant_bounds: None,
            }],
        };
        let features = parse_bed_files(&multi_bed_config, None).unwrap();
        let json_features = serde_json::to_string_pretty(&features).unwrap();
        // print the json_features to stdout for inspection
        println!("{}", &json_features);
        assert!(!features.is_empty());
        assert!(features
            .values()
            .all(|doc| doc.primary_type.starts_with("win")));
        assert!(features
            .values()
            .any(|doc| doc.attributes.as_ref().is_some_and(|attrs| {
                attrs
                    .iter()
                    .any(|attr| attr.key == "gc" || attr.key == "gc_mean")
            })));
    }
}
