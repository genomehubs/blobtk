use std::cmp::{self, Ordering};
use std::collections::HashSet;
use std::f64::consts::PI;

use serde;
use serde::{Deserialize, Serialize};
use svg::node::element::{Circle, Group, Line, Path, Rectangle, Text};
use svg::Document;
use titlecase::titlecase;

use crate::blobdir::{self, BuscoGene};
use crate::cli::ScoreType;
use crate::plot::axis::Scale;
use crate::plot::component::LegendAlignment;

use super::axis::{TickOptions, TickStatus};
use super::component::{
    arc_path, legend_group, path_axis_major, path_axis_minor, path_gridline_major,
    path_gridline_minor, polar_to_path, set_axis_ticks, set_axis_ticks_circular, LegendEntry,
    LegendShape, RadialTick, Tick,
};
use super::style::{path_filled, path_open, path_partial};
use crate::cli;
use crate::utils::{
    self, compact_float, format_pct, format_si, linear_scale, linear_scale_float, log_scale,
    sqrt_scale,
};

// Scaffold colors
const COLOR_SCAFFOLD_LENGTH: &str = "#999999";
const COLOR_SCAFFOLD_LENGTH_OUTLINE: &str = "#666666";
const COLOR_SCAFFOLD_COUNT: &str = "#dddddd";

// Composition colors
const COLOR_GC: &str = "#1f78b4";
const COLOR_GC_MIN: &str = "#a6cee3";
const COLOR_AT: &str = "#a6cee3";
const COLOR_N: &str = "#ffffff";

// Key metrics colors
const COLOR_LONGEST: &str = "#e31a1c";
const COLOR_N50: &str = "#ff7f00";
const COLOR_N90: &str = "#fdbf6f";

// Reference colors
const COLOR_REF_LENGTH: &str = "#cab2d6";
const COLOR_REF_OUTLINE: &str = "#6a3d9a";

// BUSCO colors - primary assembly
const COLOR_BUSCO_COMPLETE: &str = "#33a02c";
const COLOR_BUSCO_FRAGMENTED: &str = "#a3e27f";
const COLOR_BUSCO_DUPLICATED: &str = "#20641b";

// BUSCO colors - reference assembly (purple)
const COLOR_REF_BUSCO_COMPLETE: &str = "#6a3d9a";
const COLOR_REF_BUSCO_FRAGMENTED: &str = "#cab2d6";
const COLOR_REF_BUSCO_DUPLICATED: &str = "#4a235a";

#[derive(Serialize, Deserialize, Debug)]
pub struct SummaryStats {
    #[serde(with = "compact_float")]
    min: f64,
    #[serde(with = "compact_float")]
    max: f64,
    #[serde(with = "compact_float")]
    mean: f64,
}

impl SummaryStats {
    pub fn min(&self) -> f64 {
        self.min
    }
    pub fn max(&self) -> f64 {
        self.max
    }
    pub fn mean(&self) -> f64 {
        self.mean
    }
}

/// Data source information (FASTA, BUSCO, or BlobDir)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DataSource {
    /// Path to FASTA file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fasta: Option<String>,
    /// Path to BUSCO file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub busco: Option<String>,
    /// Path to BlobDir
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blobdir: Option<String>,
}

impl DataSource {
    pub fn is_empty(&self) -> bool {
        self.fasta.is_none() && self.busco.is_none() && self.blobdir.is_none()
    }
}

/// User-supplied parameters and data sources
#[derive(Serialize, Deserialize, Debug)]
pub struct Parameters {
    /// Maximum span value supplied by user (used for genome-size adjustment)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_span: Option<usize>,
    /// Maximum scaffold length supplied by user (used for scaffold adjustment)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_scaffold: Option<usize>,
    /// Source BlobDir or FASTA file for this assembly
    #[serde(skip_serializing_if = "DataSource::is_empty")]
    pub source: DataSource,
    /// Reference BlobDir or FASTA file (if provided)
    #[serde(skip_serializing_if = "DataSource::is_empty")]
    pub reference: DataSource,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SnailStats {
    id: String,
    #[serde(rename = "assembly")]
    span: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<Parameters>,
    #[serde(rename = "auN")]
    aun: usize,
    #[serde(rename = "auNn")]
    aun_n: usize,
    #[serde(rename = "rauN")]
    raun: f64,
    /// Snail score
    #[serde(rename = "rauNn")]
    raun_n: f64,
    /// Snail score adjusted for genome size (max_span)
    #[serde(rename = "rauNn-g", skip_serializing_if = "Option::is_none")]
    raun_ng: Option<f64>,
    /// Snail score adjusted for both genome size and longest scaffold length
    #[serde(rename = "rauNn-gs", skip_serializing_if = "Option::is_none")]
    raun_ngs: Option<f64>,
    /// Snail score adjusted for genome size, penalising assemblies larger or smaller than max_span
    #[serde(rename = "rauNn-g-absolute", skip_serializing_if = "Option::is_none")]
    raun_ng_absolute: Option<f64>,
    /// Snail score adjusted for genome size and scaffold length, penalising assemblies larger or smaller than max_span and max_scaffold
    #[serde(rename = "rauNn-gs-absolute", skip_serializing_if = "Option::is_none")]
    raun_ngs_absolute: Option<f64>,
    #[serde(rename = "ATGC")]
    atgc: usize,
    #[serde(rename = "GC", with = "compact_float")]
    gc_proportion: f64,
    #[serde(rename = "AT", with = "compact_float")]
    at_proportion: f64,
    n_proportion: f64,
    #[serde(rename = "N")]
    n: usize,
    #[serde(rename = "binned_GCs")]
    binned_gcs: Vec<SummaryStats>,
    #[serde(rename = "binned_Ns")]
    binned_ns: Vec<SummaryStats>,
    busco_complete: usize,
    busco_fragmented: usize,
    busco_duplicated: usize,
    busco_total: usize,
    busco_lineage: String,
    record_type: String,
    scaffolds: Vec<usize>,
    scaffold_count: usize,
    binned_scaffold_lengths: Vec<usize>,
    binned_scaffold_counts: Vec<usize>,
}

impl SnailStats {
    pub fn span(&self) -> usize {
        self.span
    }
    pub fn max_span(&self) -> Option<usize> {
        self.parameters.as_ref().and_then(|p| p.max_span)
    }
    pub fn max_scaffold(&self) -> Option<usize> {
        self.parameters.as_ref().and_then(|p| p.max_scaffold)
    }
    pub fn aun(&self) -> usize {
        self.aun
    }
    pub fn aun_n(&self) -> usize {
        self.aun_n
    }
    pub fn raun(&self) -> f64 {
        self.raun
    }
    /// Base snail assembly quality score (unadjusted)
    pub fn raun_n(&self) -> f64 {
        self.raun_n
    }

    pub fn calculate_genome_adjusted_scores(
        &mut self,
        max_span: Option<usize>,
        max_scaffold: Option<usize>,
    ) {
        let span = self.span() as f64;
        self.parameters.as_mut().map(|p| p.max_span = max_span);
        self.parameters
            .as_mut()
            .map(|p| p.max_scaffold = max_scaffold);
        if let Some(max) = max_span {
            if span > max as f64 {
                // ignore max_span if it's smaller than the actual span
                self.raun_ng = Some(self.raun_n());
                // set absolute score to penalise larger assemblies
                self.raun_ng_absolute = Some(self.raun_n() * (max as f64 / span));
            } else {
                self.raun_ng = Some(self.raun_n() * (span / max as f64));
                self.raun_ng_absolute = self.raun_ng();
            }
        } else if max_scaffold.is_some() {
            // if max_span is not set but max_scaffold is, still calculate raun_ng to adjust for genome size without penalising larger assemblies
            self.raun_ng = Some(self.raun_n());
            self.raun_ng_absolute = Some(self.raun_n());
        }
        if let Some(raun_ng_val) = self.raun_ng {
            if let Some(max) = max_scaffold {
                if self.scaffolds()[0] as f64 > max as f64 {
                    // ignore max_scaffold if it's smaller than the actual longest scaffold
                    self.raun_ngs = self.raun_ng();
                    // set absolute score to penalise larger scaffolds
                    self.raun_ngs_absolute =
                        Some(raun_ng_val * (max as f64 / self.scaffolds()[0] as f64));
                } else {
                    self.raun_ngs = Some(raun_ng_val * (self.scaffolds()[0] as f64 / max as f64));
                    self.raun_ngs_absolute = self.raun_ngs();
                }
            }
        }
    }

    pub fn raun_ng(&self) -> Option<f64> {
        self.raun_ng
    }
    pub fn raun_ngs(&self) -> Option<f64> {
        self.raun_ngs
    }
    pub fn raun_ng_absolute(&self) -> Option<f64> {
        self.raun_ng_absolute
    }
    pub fn raun_ngs_absolute(&self) -> Option<f64> {
        self.raun_ngs_absolute
    }

    pub fn atgc(&self) -> usize {
        self.atgc
    }
    pub fn n(&self) -> usize {
        self.n
    }
    pub fn binned_gcs(&self) -> &Vec<SummaryStats> {
        &self.binned_gcs
    }
    pub fn binned_ns(&self) -> &Vec<SummaryStats> {
        &self.binned_ns
    }
    pub fn scaffolds(&self) -> &Vec<usize> {
        &self.scaffolds
    }
    pub fn scaffold_count(&self) -> usize {
        self.scaffold_count
    }
    pub fn binned_scaffold_lengths(&self) -> &Vec<usize> {
        &self.binned_scaffold_lengths
    }
    pub fn binned_scaffold_counts(&self) -> &Vec<usize> {
        &self.binned_scaffold_counts
    }
    pub fn busco_complete(&self) -> usize {
        self.busco_complete
    }
    pub fn busco_fragmented(&self) -> usize {
        self.busco_fragmented
    }
    pub fn busco_duplicated(&self) -> usize {
        self.busco_duplicated
    }
    pub fn busco_total(&self) -> usize {
        self.busco_total
    }
    pub fn busco_lineage(&self) -> &String {
        &self.busco_lineage
    }
    pub fn record_type(&self) -> &String {
        &self.record_type
    }
}

fn count_buscos(
    busco_values: &Vec<BuscoGene>,
    busco_frag: &mut HashSet<String>,
    busco_list: &mut HashSet<String>,
    busco_dup: &mut HashSet<String>,
) {
    for busco in busco_values.clone().into_iter() {
        let busco_id = busco.id;
        if busco.status == "Fragmented" {
            busco_frag.insert(busco_id.clone());
        } else {
            if busco_list.contains(&busco_id) {
                busco_dup.insert(busco_id.clone());
            }
            busco_list.insert(busco_id);
        }
    }
}

pub fn snail_stats(
    length_values: &Vec<usize>,
    gc_values: &Vec<f64>,
    n_vals: &Option<Vec<f64>>,
    ncount_values: &Vec<usize>,
    busco_values: &Vec<Vec<blobdir::BuscoGene>>,
    busco_total: Option<usize>,
    busco_lineage: Option<String>,
    id: String,
    record_type: String,
    options: &cli::PlotOptions,
    source: DataSource,
    reference: DataSource,
) -> Result<SnailStats, anyhow::Error> {
    let span = length_values.iter().sum();
    let sum_of_squares: usize = length_values.iter().map(|&x| x * x).sum();
    let sum_of_squares_atgc: usize = length_values
        .iter()
        .zip(ncount_values.iter())
        .map(|(&len, &n)| {
            let atgc = len.saturating_sub(n);
            atgc * atgc
        })
        .sum::<usize>();
    let aun = sum_of_squares / span;
    let aun_n = sum_of_squares_atgc / span;
    let n = ncount_values.iter().sum();
    let mut new_vals = vec![];
    let busco_total = busco_total.unwrap_or_default();
    let busco_lineage = match busco_lineage {
        Some(lineage) => lineage,
        None => "".to_string(),
    };
    let n_values = match n_vals {
        Some(vals) => vals,
        None => {
            for (i, length) in length_values.iter().enumerate() {
                new_vals.push(ncount_values[i] as f64 / *length as f64);
            }
            &new_vals
        }
    };
    let atgc = span - n;
    let segment = span / options.segments;
    let order = utils::indexed_sort(length_values);
    let raun = aun as f64 / length_values[order[0]] as f64;
    let raun_n = aun_n as f64 / length_values[order[0]] as f64;
    // TODO: check span > segments
    let mut position: usize = 0;
    let mut binned_gcs: Vec<SummaryStats> = vec![];
    let mut binned_ns: Vec<SummaryStats> = vec![];
    let mut busco_list = HashSet::new();
    let mut busco_frag = HashSet::new();
    let mut busco_dup = HashSet::new();
    let mut scaffold_index: usize = 0;
    let mut scaffold_sum: usize = length_values[order[scaffold_index]];
    let mut gc_span = gc_values[order[scaffold_index]]
        * ((length_values[order[scaffold_index]] - ncount_values[order[scaffold_index]]) as f64);
    let mut at_span = (1.0 - gc_values[order[scaffold_index]])
        * ((length_values[order[scaffold_index]] - ncount_values[order[scaffold_index]]) as f64);
    let mut n_span = ncount_values[order[scaffold_index]];
    if !busco_values.is_empty() {
        count_buscos(
            &busco_values[order[scaffold_index]],
            &mut busco_frag,
            &mut busco_list,
            &mut busco_dup,
        );
    }

    let mut binned_scaffold_lengths: Vec<usize> = vec![];
    let mut binned_scaffold_counts: Vec<usize> = vec![];
    for _ in 0..options.segments {
        position += segment;
        let mut gcs: Vec<f64> = vec![gc_values[order[scaffold_index]] * 100.0];
        let mut ns: Vec<f64> = vec![n_values[order[scaffold_index]] * 100.0];
        while scaffold_sum < position {
            scaffold_index += 1;
            scaffold_sum += length_values[order[scaffold_index]];
            gcs.push(gc_values[order[scaffold_index]] * 100.0);
            ns.push(n_values[order[scaffold_index]] * 100.0);

            gc_span += gc_values[order[scaffold_index]]
                * ((length_values[order[scaffold_index]] - ncount_values[order[scaffold_index]])
                    as f64);
            at_span += (1.0 - gc_values[order[scaffold_index]])
                * ((length_values[order[scaffold_index]] - ncount_values[order[scaffold_index]])
                    as f64);
            n_span += ncount_values[order[scaffold_index]];
            if !busco_values.is_empty() {
                count_buscos(
                    &busco_values[order[scaffold_index]],
                    &mut busco_frag,
                    &mut busco_list,
                    &mut busco_dup,
                );
            }
        }
        binned_scaffold_counts.push(scaffold_index + 1);
        binned_scaffold_lengths.push(length_values[order[scaffold_index]]);
        gcs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        binned_gcs.push(SummaryStats {
            min: gcs[0],
            max: gcs[gcs.len() - 1],
            mean: gcs.iter().sum::<f64>() / gcs.len() as f64,
        });
        ns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        binned_ns.push(SummaryStats {
            min: ns[0],
            max: ns[ns.len() - 1],
            mean: ns.iter().sum::<f64>() / ns.len() as f64,
        });
    }
    Ok(SnailStats {
        span,
        parameters: if options.max_span.is_some()
            || options.max_scaffold.is_some()
            || !source.is_empty()
            || !reference.is_empty()
        {
            Some(Parameters {
                max_span: options.max_span,
                max_scaffold: options.max_scaffold,
                source,
                reference,
            })
        } else {
            None
        },
        aun,
        aun_n,
        raun,
        raun_n,
        raun_ng: None,
        raun_ngs: None,
        raun_ng_absolute: None,
        raun_ngs_absolute: None,
        atgc,
        gc_proportion: gc_span / span as f64,
        at_proportion: at_span / span as f64,
        n_proportion: n_span as f64 / span as f64,
        n,
        binned_gcs,
        binned_ns,
        scaffolds: vec![length_values[order[0]]],
        scaffold_count: length_values.len(),
        busco_complete: busco_list.len(),
        busco_duplicated: busco_dup.len(),
        busco_fragmented: busco_frag.len(),
        busco_total,
        busco_lineage,
        binned_scaffold_lengths,
        binned_scaffold_counts,
        id,
        record_type,
    })
}

pub fn scaffold_stats_legend(snail_stats: &SnailStats, options: &cli::PlotOptions) -> Group {
    let mut entries = vec![];
    let precision = options.significant_digits;
    let rounding = options.rounding.clone();
    let scaffold_count = format_si(
        &(snail_stats.scaffold_count() as f64),
        precision,
        rounding.clone(),
    );
    let scaffold_length = format_si(&(snail_stats.span() as f64), precision, rounding.clone());
    let aun = format_si(&(snail_stats.aun() as f64), precision, rounding.clone());
    let longest_scaffold = format_si(
        &(snail_stats.scaffolds()[0] as f64),
        precision,
        rounding.clone(),
    );
    let n50_bin = (options.segments / 2) - 1;
    let n90_bin = (options.segments * 9 / 10) - 1;
    let n50_length = format_si(
        &(snail_stats.binned_scaffold_lengths()[n50_bin] as f64),
        precision,
        rounding.clone(),
    );
    let n90_length = format_si(
        &(snail_stats.binned_scaffold_lengths()[n90_bin] as f64),
        precision,
        rounding.clone(),
    );
    let record = snail_stats.record_type();
    entries.push(LegendEntry {
        title: format!("Log10 {} count (total {})", record, scaffold_count),
        color: Some(COLOR_SCAFFOLD_COUNT.to_string()),
        ..Default::default()
    });
    entries.push(LegendEntry {
        title: format!(
            "{} length (total {} | auN {})",
            titlecase(record),
            scaffold_length,
            aun
        ),
        color: Some(COLOR_SCAFFOLD_LENGTH.to_string()),
        ..Default::default()
    });
    entries.push(LegendEntry {
        title: format!("Longest {} ({})", record, longest_scaffold),
        color: Some(COLOR_LONGEST.to_string()),
        ..Default::default()
    });
    entries.push(LegendEntry {
        title: format!("N50 length ({})", n50_length),
        color: Some(COLOR_N50.to_string()),
        ..Default::default()
    });
    entries.push(LegendEntry {
        title: format!("N90 length ({})", n90_length),
        color: Some(COLOR_N90.to_string()),
        ..Default::default()
    });

    let title = format!("{} statistics", titlecase(record));
    legend_group(title, entries, None, 1, LegendAlignment::Start)
}

pub fn composition_stats_legend(snail_stats: &SnailStats, options: &cli::PlotOptions) -> Group {
    let mut entries = vec![];
    let digits = options.significant_digits;
    let precision = options.decimal_precision;
    let rounding = options.rounding.clone();
    let show_numbers = options.show_numbers;
    let gc_prop = if show_numbers {
        format_si(
            &(snail_stats.gc_proportion * snail_stats.span as f64),
            digits,
            rounding.clone(),
        )
    } else {
        format_pct(
            &(snail_stats.gc_proportion * 100.0),
            precision,
            rounding.clone(),
        )
    };
    let at_prop = if show_numbers {
        format_si(
            &(snail_stats.at_proportion * snail_stats.span as f64),
            digits,
            rounding.clone(),
        )
    } else {
        format_pct(
            &(snail_stats.at_proportion * 100.0),
            precision,
            rounding.clone(),
        )
    };
    let n_prop = if show_numbers {
        format_si(
            &(snail_stats.n_proportion * snail_stats.span as f64),
            digits,
            rounding.clone(),
        )
    } else {
        format_pct(
            &(snail_stats.n_proportion * 100.0),
            precision,
            rounding.clone(),
        )
    };
    entries.push(LegendEntry {
        title: format!("GC ({})", gc_prop),
        color: Some(COLOR_GC.to_string()),
        ..Default::default()
    });
    entries.push(LegendEntry {
        title: format!("AT ({})", at_prop),
        color: Some(COLOR_AT.to_string()),
        ..Default::default()
    });
    entries.push(LegendEntry {
        title: format!("N ({})", n_prop),
        color: Some(COLOR_N.to_string()),
        ..Default::default()
    });

    let title = "Composition".to_string();
    legend_group(title, entries, None, 1, LegendAlignment::Start)
}

pub fn scale_stats_legend(snail_stats: &SnailStats, options: &cli::PlotOptions) -> Group {
    let mut entries = vec![];
    let digits = options.significant_digits;
    let rounding = options.rounding.clone();
    let max_span = match options.max_span {
        Some(span) => span,
        None => snail_stats.span(),
    };
    let max_scaffold = match options.max_scaffold {
        Some(scaffold_length) => scaffold_length,
        None => snail_stats.scaffolds()[0],
    };
    let circ_prop = format_si(&(max_span as f64), digits, rounding.clone());
    let rad_prop = format_si(&(max_scaffold as f64), digits, rounding.clone());
    entries.push(LegendEntry {
        title: circ_prop.to_string(),
        shape: Some(LegendShape::Circumference),
        ..Default::default()
    });
    entries.push(LegendEntry {
        title: rad_prop.to_string(),
        shape: Some(LegendShape::Radius),
        ..Default::default()
    });

    let title = "Scale".to_string();
    legend_group(title, entries, None, 1, LegendAlignment::Start)
}

pub fn dataset_name_legend(snail_stats: &SnailStats, _: &cli::PlotOptions) -> Group {
    let entries = vec![];

    let title = format!("Dataset: {}", snail_stats.id);
    legend_group(title, entries, None, 1, LegendAlignment::Start)
}

pub fn busco_stats_legend(snail_stats: &SnailStats, options: &cli::PlotOptions) -> Group {
    let mut entries = vec![];
    let precision = options.decimal_precision;
    let rounding = options.rounding.clone();
    let show_numbers = options.show_numbers || options.busco_numbers;
    let comp_prop = if show_numbers {
        snail_stats.busco_complete.to_string()
    } else {
        format_pct(
            &(snail_stats.busco_complete as f64 / snail_stats.busco_total as f64 * 100.0),
            precision,
            rounding.clone(),
        )
    };
    let dup_prop = if show_numbers {
        snail_stats.busco_duplicated.to_string()
    } else {
        format_pct(
            &(snail_stats.busco_duplicated as f64 / snail_stats.busco_total as f64 * 100.0),
            precision,
            rounding.clone(),
        )
    };
    let frag_prop = if show_numbers {
        snail_stats.busco_fragmented.to_string()
    } else {
        format_pct(
            &(snail_stats.busco_fragmented as f64 / snail_stats.busco_total as f64 * 100.0),
            precision,
            rounding.clone(),
        )
    };
    let missing_prop = if show_numbers {
        (snail_stats.busco_total - snail_stats.busco_complete - snail_stats.busco_fragmented)
            .to_string()
    } else {
        format_pct(
            &((snail_stats.busco_total - snail_stats.busco_complete - snail_stats.busco_fragmented)
                as f64
                / snail_stats.busco_total as f64
                * 100.0),
            precision,
            rounding.clone(),
        )
    };
    let subtitle = format!(
        "{} ({})",
        snail_stats.busco_lineage,
        snail_stats.busco_total()
    );
    entries.push(LegendEntry {
        title: format!("Comp. ({})", comp_prop),
        color: Some(COLOR_BUSCO_COMPLETE.to_string()),
        ..Default::default()
    });
    entries.push(LegendEntry {
        title: format!("Dupl. ({})", dup_prop),
        color: Some(COLOR_BUSCO_DUPLICATED.to_string()),
        ..Default::default()
    });
    entries.push(LegendEntry {
        title: format!("Frag. ({})", frag_prop),
        color: Some(COLOR_BUSCO_FRAGMENTED.to_string()),
        ..Default::default()
    });
    entries.push(LegendEntry {
        title: format!("Missing ({})", missing_prop),
        color: Some(COLOR_N.to_string()),
        ..Default::default()
    });

    let title = "BUSCO".to_string();
    legend_group(title, entries, Some(subtitle), 2, LegendAlignment::Start)
}

/// Configuration for snail plot dimensions and scales
#[derive(Clone)]
struct SnailPlotConfig {
    max_span: usize,
    max_scaffold: usize,
    ratio: f64,
    radius: f64,
    outer_radius: f64,
    bin_count: usize,
    min_value: usize,
    max_radians: f64,
    n50_index: usize,
    n90_index: usize,
    as_badge: bool,
}

impl SnailPlotConfig {
    fn new(
        snail_stats: &SnailStats,
        ref_snail_stats: &Option<SnailStats>,
        options: &cli::PlotOptions,
        max_span: Option<usize>,
        max_scaffold: Option<usize>,
    ) -> Self {
        let max_span = match max_span {
            Some(span) => {
                if span > snail_stats.span() {
                    span
                } else {
                    snail_stats.span()
                }
            }
            None => snail_stats.span(),
        };
        let max_scaffold = match max_scaffold {
            Some(scaffold_length) => {
                if scaffold_length > snail_stats.scaffolds()[0] {
                    scaffold_length
                } else {
                    snail_stats.scaffolds()[0]
                }
            }
            None => snail_stats.scaffolds()[0],
        };
        let mut ratio = 1.0;
        if let Some(ref_stats) = ref_snail_stats {
            ratio = ref_stats.span() as f64 / snail_stats.span() as f64;
        }

        let radius = 375.0;
        let outer_radius = 450.0;
        let bin_count = snail_stats.binned_scaffold_lengths().len();
        let min_scaffold = snail_stats.binned_scaffold_lengths()[bin_count - 1];
        let mut magnitude = (min_scaffold as f64).log10() as u32;
        if magnitude > 1 {
            magnitude -= 1;
        }
        let min_value = 10u32.pow(magnitude) as usize;

        let circle_radians = PI * 1.9999999;
        let max_radians = circle_radians * snail_stats.span() as f64 / max_span as f64;
        let n50_index = (bin_count / 2) - 1;
        let n90_index = (9 * bin_count / 10) - 1;

        SnailPlotConfig {
            max_span,
            max_scaffold,
            ratio,
            radius,
            outer_radius,
            bin_count,
            min_value,
            max_radians,
            n50_index,
            n90_index,
            as_badge: options.badge,
        }
    }
}

/// Polar coordinates for all data series in the snail plot
#[derive(Clone)]
struct PolarCoordinates {
    scaffold: Vec<Vec<f64>>,
    reference: Vec<Vec<f64>>,
    count: Vec<Vec<f64>>,
    longest: Vec<Vec<f64>>,
    n50: Vec<Vec<f64>>,
    n90: Vec<Vec<f64>>,
    gc: Vec<Vec<f64>>,
    gc_max: Vec<Vec<f64>>,
    gc_min: Vec<Vec<f64>>,
    at: Vec<Vec<f64>>,
    inner_n: Vec<Vec<f64>>,
    outer_n: Vec<Vec<f64>>,
    inner_n_max: Vec<Vec<f64>>,
    outer_n_max: Vec<Vec<f64>>,
    show_longest: bool,
    scaf_count_domain: [usize; 2],
    scaf_count_range: [f64; 2],
}

impl PolarCoordinates {
    fn new(
        snail_stats: &SnailStats,
        ref_snail_stats: &Option<SnailStats>,
        config: &SnailPlotConfig,
        options: &cli::PlotOptions,
    ) -> Self {
        let length_scale_function = match options.scale_function {
            Scale::LINEAR => linear_scale,
            Scale::SQRT => sqrt_scale,
            Scale::LOG => log_scale,
        };

        let mut coords = PolarCoordinates {
            scaffold: vec![],
            reference: vec![],
            count: vec![],
            longest: vec![],
            n50: vec![],
            n90: vec![],
            gc: vec![],
            gc_max: vec![],
            gc_min: vec![],
            at: vec![],
            inner_n: vec![],
            outer_n: vec![],
            inner_n_max: vec![],
            outer_n_max: vec![],
            show_longest: false,
            scaf_count_domain: [1, 10_000_000_000],
            scaf_count_range: [0.0, config.radius],
        };

        let scaled_n50 = length_scale_function(
            snail_stats.binned_scaffold_lengths()[config.n50_index],
            &[config.min_value, config.max_scaffold],
            &[config.radius, 0.0],
        );
        let scaled_n90 = length_scale_function(
            snail_stats.binned_scaffold_lengths()[config.n90_index],
            &[config.min_value, config.max_scaffold],
            &[config.radius, 0.0],
        );

        for i in 0..config.bin_count {
            let angle = linear_scale(
                i + 1,
                &[0, config.bin_count],
                &[-PI / 2.0, config.max_radians - PI / 2.0],
            );

            // scaffold lengths
            coords.scaffold.push(vec![
                length_scale_function(
                    snail_stats.binned_scaffold_lengths()[i],
                    &[config.min_value, config.max_scaffold],
                    &[config.radius, 0.0],
                ),
                angle,
            ]);

            // reference scaffold lengths
            if let Some(ref_stats) = ref_snail_stats {
                let ref_angle = linear_scale(
                    i + 1,
                    &[0, config.bin_count],
                    &[-PI / 2.0, config.max_radians * config.ratio - PI / 2.0],
                );
                coords.reference.push(vec![
                    length_scale_function(
                        ref_stats.binned_scaffold_lengths()[i],
                        &[config.min_value, config.max_scaffold],
                        &[config.radius, 0.0],
                    ),
                    ref_angle,
                ]);
            } else {
                coords.count.push(vec![
                    log_scale(
                        snail_stats.binned_scaffold_counts()[i],
                        &coords.scaf_count_domain,
                        &coords.scaf_count_range,
                    ),
                    angle,
                ]);
            }

            // gc
            let gc_stats = &snail_stats.binned_gcs()[i];
            coords.gc.push(vec![
                linear_scale_float(
                    gc_stats.mean(),
                    &[0.0, 100.0],
                    &[config.radius, config.outer_radius],
                ),
                angle,
            ]);
            coords.gc_max.push(vec![
                linear_scale_float(
                    gc_stats.max(),
                    &[0.0, 100.0],
                    &[config.radius, config.outer_radius],
                ),
                angle,
            ]);
            coords.gc_min.push(vec![
                linear_scale_float(
                    gc_stats.min(),
                    &[0.0, 100.0],
                    &[config.radius, config.outer_radius],
                ),
                angle,
            ]);

            // at
            coords.at.push(vec![
                linear_scale_float(
                    100.0 - gc_stats.mean(),
                    &[0.0, 100.0],
                    &[config.outer_radius, config.radius],
                ),
                angle,
            ]);

            // n
            let n_stats = &snail_stats.binned_ns()[i];
            coords.inner_n.push(vec![
                linear_scale_float(
                    n_stats.mean() / 2.0,
                    &[0.0, 100.0],
                    &[config.radius, config.outer_radius],
                ),
                angle,
            ]);
            coords.inner_n_max.push(vec![
                linear_scale_float(
                    n_stats.max() / 2.0,
                    &[0.0, 100.0],
                    &[config.radius, config.outer_radius],
                ),
                angle,
            ]);
            coords.outer_n.push(vec![
                linear_scale_float(
                    n_stats.mean() / 2.0,
                    &[0.0, 100.0],
                    &[config.outer_radius, config.radius],
                ),
                angle,
            ]);
            coords.outer_n_max.push(vec![
                linear_scale_float(
                    n_stats.max() / 2.0,
                    &[0.0, 100.0],
                    &[config.outer_radius, config.radius],
                ),
                angle,
            ]);

            // longest scaffold
            if snail_stats.binned_scaffold_lengths()[i] == config.max_scaffold {
                coords.longest.push(vec![0.0, angle]);
                coords.show_longest = true;
            }

            // n50/n90
            if i <= config.n90_index {
                if i <= config.n50_index {
                    coords.n50.push(vec![scaled_n50, angle]);
                }
                coords.n90.push(vec![scaled_n90, angle]);
            }
        }

        coords
    }
}

struct PlotColors {
    scaffold_length: &'static str,
    scaffold_length_outline: &'static str,
    scaffold_count: &'static str,
    gc: &'static str,
    gc_min: &'static str,
    at: &'static str,
    n: &'static str,
    longest: &'static str,
    n50: &'static str,
    n90: &'static str,
    ref_length: &'static str,
    ref_outline: &'static str,
}

struct PlotPaths {
    scaf_length: Path,
    scaf_length_outline: Path,
    scaf_count: Path,
    gc_prop: Path,
    gc_prop_max: Path,
    gc_prop_min: Path,
    at_prop: Path,
    n_prop_inner: Path,
    n_prop_outer: Path,
    n_prop_inner_max: Path,
    n_prop_outer_max: Path,
    longest_arc: Path,
    n50_arc: Path,
    n90_arc: Path,
    ref_length: Path,
    ref_length_outline: Group,
    n50_arc_outline: Path,
    n90_arc_outline: Path,
    longest_arc_outline: Path,
    inner_axis: Line,
    inner: Path,
    outer: Path,
    major_count_gridline: Group,
    major_length_gridline: Group,
    major_tick: Group,
    minor_tick: Group,
    major_length_tick: Group,
    minor_length_tick: Group,
}

impl PlotPaths {
    fn new(
        snail_stats: &SnailStats,
        ref_snail_stats: &Option<SnailStats>,
        config: &SnailPlotConfig,
        polar: &PolarCoordinates,
        options: &cli::PlotOptions,
        major_ticks: Vec<RadialTick>,
        minor_ticks: Vec<RadialTick>,
        major_length_ticks: Vec<Tick>,
        minor_length_ticks: Vec<Tick>,
        colors: PlotColors,
    ) -> Self {
        let polar_axis_coords: Vec<Vec<f64>> = vec![];

        let scaf_length_data = polar_to_path(
            &polar.scaffold,
            config.radius,
            config.bin_count,
            config.max_radians,
        );
        let ref_length_data = polar_to_path(
            &polar.reference,
            config.radius,
            config.bin_count,
            config.max_radians,
        );
        let scaf_count_data =
            polar_to_path(&polar.count, 0.0, config.bin_count, config.max_radians);
        let gc_prop_data = polar_to_path(
            &polar.gc,
            config.radius,
            config.bin_count,
            config.max_radians,
        );
        let gc_prop_max_data = polar_to_path(
            &polar.gc_max,
            config.radius,
            config.bin_count,
            config.max_radians,
        );
        let gc_prop_min_data = polar_to_path(
            &polar.gc_min,
            config.radius,
            config.bin_count,
            config.max_radians,
        );
        let at_prop_data = polar_to_path(
            &polar.at,
            config.outer_radius,
            config.bin_count,
            config.max_radians,
        );
        let n_prop_inner_data = polar_to_path(
            &polar.inner_n,
            config.radius,
            config.bin_count,
            config.max_radians,
        );
        let n_prop_outer_data = polar_to_path(
            &polar.outer_n,
            config.outer_radius,
            config.bin_count,
            config.max_radians,
        );
        let n_prop_inner_max_data = polar_to_path(
            &polar.inner_n_max,
            config.radius,
            config.bin_count,
            config.max_radians,
        );
        let n_prop_outer_max_data = polar_to_path(
            &polar.outer_n_max,
            config.outer_radius,
            config.bin_count,
            config.max_radians,
        );
        let longest_arc_data = polar_to_path(
            &polar.longest,
            config.radius,
            config.bin_count,
            config.max_radians,
        );
        let n50_arc_data = polar_to_path(
            &polar.n50,
            config.radius,
            config.bin_count,
            config.max_radians,
        );
        let n90_arc_data = polar_to_path(
            &polar.n90,
            config.radius,
            config.bin_count,
            config.max_radians,
        );
        let axis_arc_data = polar_to_path(
            &polar_axis_coords,
            config.radius,
            config.bin_count,
            config.max_radians,
        );
        let outer_axis_arc_data = polar_to_path(
            &polar_axis_coords,
            config.outer_radius,
            config.bin_count,
            config.max_radians,
        );
        let longest_arc_outline_data = polar_to_path(
            &polar.longest,
            config.radius,
            config.bin_count,
            config.max_radians,
        );
        let n50_arc_outline_data = polar_to_path(
            &polar.n50,
            config.radius,
            config.bin_count,
            config.max_radians,
        );
        let n90_arc_outline_data = polar_to_path(
            &polar.n90,
            config.radius,
            config.bin_count,
            config.max_radians,
        );

        let scaf_length = path_filled(scaf_length_data.clone(), Some(colors.scaffold_length));
        let scaf_length_outline = if ref_snail_stats.is_some() {
            path_open(
                scaf_length_data,
                Some(colors.scaffold_length_outline),
                Some(1.5),
            )
        } else {
            Path::new()
        };
        let scaf_count = path_filled(scaf_count_data, Some(colors.scaffold_count));
        let gc_prop = path_filled(gc_prop_data, Some(colors.gc));
        let gc_prop_max = path_partial(gc_prop_max_data, Some(colors.gc), None);
        let gc_prop_min = path_partial(gc_prop_min_data, Some(colors.gc_min), None);
        let at_prop = path_filled(at_prop_data, Some(colors.at));
        let n_prop_inner = path_filled(n_prop_inner_data, Some(colors.n));
        let n_prop_outer = path_filled(n_prop_outer_data, Some(colors.n));
        let n_prop_inner_max = path_partial(n_prop_inner_max_data, Some(colors.n), Some(0.5));
        let n_prop_outer_max = path_partial(n_prop_outer_max_data, Some(colors.n), Some(0.5));

        let longest_arc = if polar.show_longest {
            path_filled(longest_arc_data, Some(colors.longest))
        } else {
            Path::new()
        };
        let n50_arc = path_filled(n50_arc_data, Some(colors.n50));
        let n90_arc = path_filled(n90_arc_data, Some(colors.n90));
        let ref_length = path_filled(ref_length_data.clone(), Some(colors.ref_length));
        // Create reference outline (styling applied in svg() if pattern needed)
        let ref_outline_path = path_open(ref_length_data, Some(colors.ref_outline), None);
        let mut ref_length_outline = Group::new().add(ref_outline_path);
        let n50_arc_outline = path_open(n50_arc_outline_data, Some(colors.n50), None);
        let n90_arc_outline = path_open(n90_arc_outline_data, Some(colors.n90), None);
        let longest_arc_outline = path_open(longest_arc_outline_data, Some(colors.longest), None);
        let inner = path_axis_major(axis_arc_data, None, None);
        let outer = path_axis_major(outer_axis_arc_data, None, None);

        let inner_axis = Line::new()
            .set("fill", "none")
            .set("stroke", "black")
            .set("stroke-width", 3)
            .set("x1", 0.0)
            .set("y1", 0.0)
            .set("x2", 0.0)
            .set("y2", -config.radius);

        // Build tick groups
        let mut major_tick = Group::new();
        for tick in major_ticks {
            major_tick = major_tick
                .add(tick.path)
                .add(if !config.as_badge {
                    tick.label
                } else {
                    Text::new()
                })
                .add(if !config.as_badge {
                    tick.outer_label
                } else {
                    Text::new()
                })
        }

        let mut minor_tick = Group::new();
        for tick in minor_ticks {
            minor_tick = minor_tick.add(tick.path)
        }

        let mut major_length_tick = Group::new();
        let mut major_length_gridline = Group::new();

        for (i, tick) in major_length_ticks.iter().enumerate() {
            let tick = tick.clone();
            let label = if !matches!(options.scale_function, Scale::LINEAR)
                && i < cmp::max(major_length_ticks.len(), 3) - 3
            {
                Text::new()
            } else {
                tick.label
            };
            major_length_tick = major_length_tick.add(tick.path).add(if !config.as_badge {
                label
            } else {
                Text::new()
            });

            if matches!(options.scale_function, Scale::LINEAR) && i == major_length_ticks.len() - 1
            {
                continue;
            }
            let arc_data = arc_path(
                -tick.position,
                None,
                -PI / 2.0,
                PI * 1.9999999 - PI / 2.0,
                options.segments,
            );
            major_length_gridline =
                major_length_gridline.add(path_gridline_minor(arc_data, Some("#ffffff")));
        }

        let mut minor_length_tick = Group::new();
        for tick in minor_length_ticks {
            minor_length_tick = minor_length_tick.add(tick.path)
        }

        let mut major_count_gridline = Group::new();
        if ref_snail_stats.is_some() {
            if config.ratio < 0.98 {
                let ref_end_path_line = Line::new()
                    .set(
                        "x1",
                        config.radius * ((config.max_radians - PI / 2.0) * config.ratio).cos(),
                    )
                    .set(
                        "y1",
                        config.radius * ((config.max_radians - PI / 2.0) * config.ratio).sin(),
                    )
                    .set(
                        "x2",
                        config.outer_radius
                            * ((config.max_radians - PI / 2.0) * config.ratio).cos(),
                    )
                    .set(
                        "y2",
                        config.outer_radius
                            * ((config.max_radians - PI / 2.0) * config.ratio).sin(),
                    )
                    .set("fill", "none")
                    .set("stroke", colors.ref_outline)
                    .set("stroke-width", 3);
                ref_length_outline =
                    ref_length_outline.add(ref_end_path_line.set("stroke", colors.ref_outline));
            }
        } else {
            let mut i = 10;
            while i <= snail_stats.scaffold_count() {
                let arc_data = arc_path(
                    log_scale(i, &polar.scaf_count_domain, &polar.scaf_count_range),
                    None,
                    -PI / 2.0,
                    PI * 1.9999999 - PI / 2.0,
                    options.segments,
                );
                major_count_gridline =
                    major_count_gridline.add(path_gridline_major(arc_data, Some("#ffffff")));
                i *= 10;
            }
        }

        PlotPaths {
            scaf_length,
            scaf_length_outline,
            scaf_count,
            gc_prop,
            gc_prop_max,
            gc_prop_min,
            at_prop,
            n_prop_inner,
            n_prop_outer,
            n_prop_inner_max,
            n_prop_outer_max,
            longest_arc,
            n50_arc,
            n90_arc,
            ref_length,
            ref_length_outline,
            n50_arc_outline,
            n90_arc_outline,
            longest_arc_outline,
            inner_axis,
            inner,
            outer,
            major_count_gridline,
            major_length_gridline,
            major_tick,
            minor_tick,
            major_length_tick,
            minor_length_tick,
        }
    }
}

struct PlotLegends {
    scaf_stats: Group,
    score: Group,
    comp_stats: Group,
    scale: Group,
    dataset: Group,
    busco_stats: Group,
    busco_plot: Group,
}

impl PlotLegends {
    fn get_score_and_type(snail_stats: &SnailStats, options: &cli::PlotOptions) -> (f64, String) {
        let score = match options.score_type {
            Some(ScoreType::Base) => snail_stats.raun_n(),
            Some(ScoreType::G) => snail_stats.raun_ng().unwrap(),
            Some(ScoreType::Gs) => snail_stats.raun_ngs().unwrap(),
            Some(ScoreType::GAbsolute) => snail_stats.raun_ng_absolute().unwrap(),
            Some(ScoreType::GsAbsolute) => snail_stats.raun_ngs_absolute().unwrap(),
            None => snail_stats.raun_n(),
        };
        let score_type = match options.score_type {
            Some(ScoreType::Base) => "Score".to_string(),
            Some(ScoreType::G) => "G-score".to_string(),
            Some(ScoreType::Gs) => "GS-score".to_string(),
            Some(ScoreType::GAbsolute) => "aG-score".to_string(),
            Some(ScoreType::GsAbsolute) => "aGS-score".to_string(),
            None => "Score".to_string(),
        };
        (score, score_type)
    }

    fn new(
        snail_stats: &SnailStats,
        ref_snail_stats: &Option<SnailStats>,
        options: &cli::PlotOptions,
        config: &SnailPlotConfig,
        busco_colors: (&str, &str, &str, &str, &str, &str),
    ) -> Self {
        let scaf_stats = scaffold_stats_legend(snail_stats, options)
            .set("transform", format!("translate({},{})", 5, 25));

        let score = if options.show_score {
            let (snail_score, score_type) = PlotLegends::get_score_and_type(snail_stats, options);
            dbg!(&score_type, snail_score);
            legend_group(
                format!("{}: {}", score_type, format_si(&snail_score, 3, None)),
                if let Some(ref_stats) = ref_snail_stats {
                    let (ref_score, _) = PlotLegends::get_score_and_type(ref_stats, options);
                    let delta = snail_score - ref_score;
                    vec![
                        LegendEntry {
                            color: Some(COLOR_REF_BUSCO_COMPLETE.to_string()),
                            title: format!(
                                "{} | Δ {}{}",
                                format_si(&ref_score, 3, None),
                                if delta >= 0.0 { "+" } else { "" },
                                format_si(&delta, 3, None)
                            ),
                            shape: None,
                            ..Default::default()
                        },
                        // format!(
                        //     "\nRef: {} | Δ: {}{}",
                        //     format_si(&ref_score, 3, None),
                        //     if delta >= 0.0 { "+" } else { "" },
                        //     format_si(&delta, 3, None)
                        // ),
                    ]
                } else {
                    vec![]
                },
                None,
                1,
                LegendAlignment::Start,
            )
            .set(
                "transform",
                format!(
                    "translate({},{})",
                    433.7,
                    if ref_snail_stats.is_some() { 25 } else { 35 }
                ),
            )
        } else {
            Group::new()
        };

        let comp_stats = composition_stats_legend(snail_stats, options)
            .set("transform", format!("translate({},{})", 835, 900));

        let scale = scale_stats_legend(snail_stats, options)
            .set("transform", format!("translate({},{})", 5, 900));

        let dataset = dataset_name_legend(snail_stats, options)
            .set("transform", format!("translate({},{})", 5, 990));

        let (busco_stats, busco_plot) = if snail_stats.busco_total() >= 1 {
            (
                busco_stats_legend(snail_stats, options)
                    .set("transform", format!("translate({},{})", 630, 25)),
                busco_plot(snail_stats, ref_snail_stats, config.as_badge, busco_colors).set(
                    "transform",
                    if config.as_badge {
                        "translate(868, 147)"
                    } else {
                        "translate(910, 170)"
                    },
                ),
            )
        } else {
            (Group::new(), Group::new())
        };

        PlotLegends {
            scaf_stats,
            score,
            comp_stats,
            scale,
            dataset,
            busco_stats,
            busco_plot,
        }
    }

    fn add_to_document(self, mut document: Document, as_badge: bool) -> Document {
        document = document
            .add(if !as_badge {
                self.scaf_stats.clone()
            } else {
                Group::new()
            })
            .add(if !as_badge {
                self.score.clone()
            } else {
                Group::new()
            })
            .add(if !as_badge {
                self.comp_stats.clone()
            } else {
                Group::new()
            })
            .add(if !as_badge {
                self.busco_stats.clone()
            } else {
                Group::new()
            })
            .add(if !as_badge {
                self.scale.clone()
            } else {
                Group::new()
            })
            .add(if !as_badge {
                self.dataset.clone()
            } else {
                Group::new()
            })
            .add(self.busco_plot.clone());
        document
    }
}

pub fn svg(
    snail_stats: &SnailStats,
    ref_snail_stats: &Option<SnailStats>,
    options: &cli::PlotOptions,
    max_span: Option<usize>,
    max_scaffold: Option<usize>,
) -> Document {
    // Create plot configuration
    let config = SnailPlotConfig::new(
        snail_stats,
        ref_snail_stats,
        options,
        max_span,
        max_scaffold,
    );

    const MAJOR_TICK_COUNT: usize = 10;
    const MINOR_TICK_COUNT: usize = 50;
    let major_ticks = set_axis_ticks_circular(
        config.bin_count,
        MAJOR_TICK_COUNT,
        TickStatus::Major,
        config.max_radians,
        config.radius,
        config.outer_radius,
        snail_stats.span(),
        TickOptions {
            label_ticks: true,
            ..Default::default()
        },
    );
    let minor_ticks = set_axis_ticks_circular(
        config.bin_count,
        MINOR_TICK_COUNT,
        TickStatus::Minor,
        config.max_radians,
        config.radius,
        config.outer_radius,
        snail_stats.span(),
        TickOptions {
            label_ticks: true,
            ..Default::default()
        },
    );
    let length_scale = match options.scale_function {
        Scale::LINEAR => "scaleLinear".to_string(),
        Scale::SQRT => "scaleSqrt".to_string(),
        Scale::LOG => "scaleLog".to_string(),
    };
    let major_length_ticks = set_axis_ticks(
        &(config.max_scaffold as f64),
        &(config.min_value as f64),
        &TickStatus::Major,
        &config.radius,
        &length_scale,
    );
    let minor_length_ticks = set_axis_ticks(
        &(config.max_scaffold as f64),
        &(config.min_value as f64),
        &TickStatus::Minor,
        &config.radius,
        &length_scale,
    );

    // Generate all polar coordinates
    let polar = PolarCoordinates::new(snail_stats, ref_snail_stats, &config, options);

    // Generate all paths
    let paths = PlotPaths::new(
        snail_stats,
        ref_snail_stats,
        &config,
        &polar,
        options,
        major_ticks,
        minor_ticks,
        major_length_ticks,
        minor_length_ticks,
        PlotColors {
            scaffold_length: COLOR_SCAFFOLD_LENGTH,
            scaffold_length_outline: COLOR_SCAFFOLD_LENGTH_OUTLINE,
            scaffold_count: COLOR_SCAFFOLD_COUNT,
            gc: COLOR_GC,
            gc_min: COLOR_GC_MIN,
            at: COLOR_AT,
            n: COLOR_N,
            longest: COLOR_LONGEST,
            n50: COLOR_N50,
            n90: COLOR_N90,
            ref_length: COLOR_REF_LENGTH,
            ref_outline: COLOR_REF_OUTLINE,
        },
    );

    // Generate all legends
    let legends = PlotLegends::new(
        snail_stats,
        ref_snail_stats,
        options,
        &config,
        (
            COLOR_BUSCO_COMPLETE,
            COLOR_BUSCO_FRAGMENTED,
            COLOR_BUSCO_DUPLICATED,
            COLOR_REF_BUSCO_COMPLETE,
            COLOR_REF_BUSCO_FRAGMENTED,
            COLOR_REF_BUSCO_DUPLICATED,
        ),
    );

    let mut group = Group::new().set("transform", "translate(500, 525)");

    group = group
        .add(if ref_snail_stats.is_some() {
            Group::new().add(paths.ref_length)
        } else {
            Group::new()
                .add(paths.scaf_count)
                .add(paths.major_count_gridline)
        })
        .add(paths.scaf_length)
        .add(paths.scaf_length_outline)
        .add(paths.gc_prop)
        .add(paths.at_prop)
        .add(paths.n_prop_inner)
        .add(paths.n_prop_outer)
        .add(paths.n_prop_inner_max)
        .add(paths.n_prop_outer_max)
        .add(paths.gc_prop_max)
        .add(paths.gc_prop_min)
        .add(paths.longest_arc)
        .add(paths.n50_arc)
        .add(paths.n90_arc)
        .add(paths.n50_arc_outline)
        .add(paths.n90_arc_outline)
        .add(paths.longest_arc_outline)
        .add(if ref_snail_stats.is_some() {
            Group::new().add(paths.ref_length_outline)
        } else {
            Group::new()
        })
        .add(paths.major_length_gridline)
        .add(paths.minor_tick)
        .add(paths.major_tick)
        .add(paths.minor_length_tick)
        .add(paths.major_length_tick)
        .add(paths.inner_axis)
        // .add(paths.outer_axis)
        .add(paths.inner)
        .add(paths.outer)
        .add(if config.ratio > 1.0 {
            Group::new().add(
                Circle::new()
                    .set("fill", "none")
                    .set("cx", 0)
                    .set("cy", 0)
                    .set("r", config.radius)
                    .set("stroke", "#000000")
                    .set("stroke-width", 3),
            )
        } else {
            Group::new()
        });

    // svg::save(options.output.as_str(), &document).unwrap();
    // let mut target = Vec::new();
    // let svg_data = svg::write(target, &document).unwrap();
    let base_document = Document::new()
        .set(
            "viewBox",
            if config.as_badge {
                (
                    (500.0 - config.outer_radius) as i64 - 2,
                    (525.0 - config.outer_radius) as i64 - 2,
                    (config.outer_radius * 2.0) as i64 + 4,
                    (config.outer_radius * 2.0) as i64 + 4,
                )
            } else {
                (0, 0, 1000, 1000)
            },
        )
        .add(if config.as_badge {
            Group::new()
                .add(
                    Circle::new()
                        .set("fill", "#ffffff")
                        .set("cx", 500)
                        .set("cy", 525)
                        .set("r", config.outer_radius),
                )
                .add(
                    Circle::new()
                        .set("fill", "#ffffff")
                        .set("cx", 500 + config.outer_radius as i64 - 82)
                        .set("cy", 525 - config.outer_radius as i64 + 72)
                        .set("r", 69),
                )
        } else {
            Group::new().add(
                Rectangle::new()
                    .set("fill", "#ffffff")
                    .set("stroke", "none")
                    .set("width", 1000)
                    .set("height", 1000),
            )
        });

    let document_with_legends = legends.add_to_document(base_document, config.as_badge);

    document_with_legends.add(group)
}

fn busco_plot(
    snail_stats: &SnailStats,
    ref_snail_stats: &Option<SnailStats>,
    as_badge: bool,
    busco_colors: (&str, &str, &str, &str, &str, &str),
) -> Group {
    let (
        color_complete,
        color_fragmented,
        color_duplicated,
        color_ref_complete,
        color_ref_fragmented,
        color_ref_duplicated,
    ) = busco_colors;
    let domain = [0.0, snail_stats.busco_total() as f64];
    let range = [-PI / 2.0, PI * 1.5];
    let inner_radius = if as_badge { 39.0 } else { 35.0 };
    let outer_radius = if as_badge { 69.0 } else { 60.0 };
    let comp_arc_data = arc_path(
        outer_radius,
        Some(inner_radius),
        -PI / 2.0,
        linear_scale_float(snail_stats.busco_complete() as f64, &domain, &range),
        1000,
    );
    let comp_arc_path = path_filled(comp_arc_data, Some(color_complete));
    let frag_arc_data = arc_path(
        outer_radius,
        Some(inner_radius),
        linear_scale_float(snail_stats.busco_complete() as f64, &domain, &range),
        linear_scale_float(
            (snail_stats.busco_fragmented() + snail_stats.busco_complete()) as f64,
            &domain,
            &range,
        ),
        1000,
    );
    let frag_arc_path = path_filled(frag_arc_data, Some(color_fragmented));
    let dup_arc_data = arc_path(
        outer_radius,
        Some(inner_radius),
        -PI / 2.0,
        linear_scale_float(snail_stats.busco_duplicated() as f64, &domain, &range),
        1000,
    );
    let dup_arc_path = path_filled(dup_arc_data, Some(color_duplicated));

    // Add inner circle for reference BUSCO if available
    let mut group = Group::new()
        .add(comp_arc_path)
        .add(frag_arc_path)
        .add(dup_arc_path);

    if let Some(ref_stats) = ref_snail_stats {
        if ref_stats.busco_total() >= 1 {
            let ref_inner_radius = if as_badge { 14.0 } else { 10.0 };
            let ref_outer_radius = if as_badge { 34.0 } else { 30.0 };
            let ref_domain = [0.0, ref_stats.busco_total() as f64];

            let ref_comp_arc_data = arc_path(
                ref_outer_radius,
                Some(ref_inner_radius),
                -PI / 2.0,
                linear_scale_float(ref_stats.busco_complete() as f64, &ref_domain, &range),
                1000,
            );
            let ref_comp_arc_path = path_filled(ref_comp_arc_data, Some(color_ref_complete));

            let ref_frag_arc_data = arc_path(
                ref_outer_radius,
                Some(ref_inner_radius),
                linear_scale_float(ref_stats.busco_complete() as f64, &ref_domain, &range),
                linear_scale_float(
                    (ref_stats.busco_fragmented() + ref_stats.busco_complete()) as f64,
                    &ref_domain,
                    &range,
                ),
                1000,
            );
            let ref_frag_arc_path = path_filled(ref_frag_arc_data, Some(color_ref_fragmented));

            let ref_dup_arc_data = arc_path(
                ref_outer_radius,
                Some(ref_inner_radius),
                -PI / 2.0,
                linear_scale_float(ref_stats.busco_duplicated() as f64, &ref_domain, &range),
                1000,
            );
            let ref_dup_arc_path = path_filled(ref_dup_arc_data, Some(color_ref_duplicated));

            group = group
                .add(ref_comp_arc_path)
                .add(ref_frag_arc_path)
                .add(ref_dup_arc_path);
        }
    }
    let major_ticks = set_axis_ticks_circular(
        1000,
        10,
        TickStatus::Major,
        2.0 * PI,
        outer_radius,
        outer_radius + 20.0,
        100,
        TickOptions {
            font_size: 14.0,
            ..Default::default()
        },
    );
    let mut major_tick_group = Group::new();
    for tick in major_ticks {
        major_tick_group =
            major_tick_group
                .add(tick.path)
                .add(if !as_badge { tick.label } else { Text::new() })
    }
    let minor_ticks = set_axis_ticks_circular(
        1000,
        50,
        TickStatus::Minor,
        2.0 * PI,
        outer_radius,
        outer_radius + 20.0,
        100,
        TickOptions {
            ..Default::default()
        },
    );
    let mut minor_tick_group = Group::new();
    for tick in minor_ticks {
        minor_tick_group = minor_tick_group.add(tick.path)
    }

    let cirular_axis_data = arc_path(outer_radius, None, -PI / 2.0, PI * 1.5, 1000);
    let circular_axis_path = path_axis_minor(cirular_axis_data, None, Some(2.0));

    let radial_axis = Line::new()
        .set("fill", "none")
        .set("stroke", "black")
        .set("stroke-width", 1)
        .set("x1", 0.0)
        .set("y1", 0.0)
        .set("x2", 0.0)
        .set("y2", -outer_radius);

    group
        .add(minor_tick_group)
        .add(major_tick_group)
        .add(radial_axis)
        .add(circular_axis_path)
}
