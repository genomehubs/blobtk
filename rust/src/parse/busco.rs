//! Functions to parse a busco full table file and return an iterator of BuscoFeature structs.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;

use anyhow;
use serde::{Deserialize, Serialize};

pub mod attributes;

use crate::error;
use crate::import::state::ImportState;
use crate::import::sync_attribute_documents;
use crate::import::EsConfig;
use crate::import::ImportOptions;
use crate::index::es::client::ElasticsearchClient;

use crate::index::es::models::documents::FeatureDocument;
use crate::index::es::models::nested_documents::NestedAttribute;
use crate::io::get_csv_reader;
use crate::parse::bed::WindowSpec;

use crate::parse::busco::attributes::{
    busco_alg_attribute, busco_core_attributes, synteny_block_attributes,
    synteny_block_feature_document, synteny_locus_attributes, synteny_locus_feature_document,
    AttributeCollector, SyntenyIndexArtifacts, SyntenyIndexMode,
};

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
    pub alg_count: Option<usize>,
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

fn feature_doc_key(feature_id: &str, sequence_id: &str) -> String {
    let key = format!("{}::{}", sequence_id, feature_id);
    key
}

fn ensure_feature_attributes(feature_document: &mut FeatureDocument) -> &mut Vec<NestedAttribute> {
    feature_document.attributes.get_or_insert_with(Vec::new)
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
        let mut unique_values = std::collections::HashSet::new();
        for result in alg_reader.into_records() {
            let record = result?;
            let key = record.get(0).unwrap().to_string();
            let value = record.get(1).unwrap().to_string();
            unique_values.insert(value.clone());
            mapping.insert(key, value);
        }
        alg_map.insert(
            alg.name.clone(),
            AlgConfig {
                name: alg.name.clone(),
                path: alg.path.clone(),
                lineage: alg.lineage.clone(),
                alg_count: Some(unique_values.len()),
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
            let categories = vec![format!("{}_unknown", lineage)];
            categories
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyntenyLocus {
    pub id: String,
    pub status: String,
    pub score: f64,
    pub sequence_id: String,
    pub assembly_id: String,
    pub taxon_id: String,
    pub start: usize,
    pub end: usize,
    pub strand: i8,
    pub length: usize,

    // Identifier for the group set or clade-specific grouping scheme that this attribute belongs to.
    // For example, "diptera_odb12_alg".
    group_set_id: String,
    // Identifier for the specific group assignment of the locus within the selected group set.
    // For example, "diptera_odb12_alg:group_1".
    group_id: String,
    // True when this nested record describes the primary group set used for block summaries.
    is_primary: bool,
    // Identifier for the contiguous block of loci assigned to the same primary group.
    // For example, "diptera_odb12_alg:group_1:block_1".
    block_id: String,
    // Number of BUSCO loci in the contiguous same-group block.
    block_size_loci: usize,
    // Relative size of the contiguous block compared to the total number of loci in the chosen window or block context.
    block_size_proportion: f64,
    // Block size rank within the chosen window or block context, for example 1 for the largest block, 2 for the second largest, etc.
    block_size_rank: usize,
    // One-based rank of the locus within its contiguous block.
    rank_within_block: usize,
    // Normalized position of the locus within its block, for example rank divided by block size.
    rank_proportion: f64,
    // Distance in loci from the locus to the nearest block edge.
    distance_to_edge: usize,
    // // Rank of the first locus in the contiguous block.
    // block_start_rank: usize,
    // // Rank of the last locus in the contiguous block.
    // block_end_rank: usize,
    // Count of immediately adjacent upstream or downstream loci in the same group before the first interruption.
    same_group_continuous: usize,
    // Count of immediately adjacent upstream or downstream loci in different groups before returning to the primary group or hitting a boundary.
    different_group_continuous: usize,
    // Total number of same-group loci in the chosen window or block context.
    same_group_total: usize,
    // Total number of different-group loci in the chosen window or block context.
    different_group_total: usize,
    // Relative count of same group loci compared to different group loci, for example same_group_total / different_group_total.
    same_to_different_ratio: f64,
    // Number of distinct non-primary groups represented among the interruptions or neighboring loci.
    distinct_different_group_count: usize,
    // Group Ids of the adjacent blocks or loci that interrupt the primary group block, for example ["group_2", "group_3"].
    adjacent_group_ids: Vec<String>,
    // // True when the number of interruptions passes a configured threshold, such as more than three.
    // interruption_threshold_flag: bool,
    // // True when the block may be truncated by a contig end, scaffold edge, or sparse BUSCO sampling.
    // is_edge_truncated: bool,
}

impl SyntenyLocus {
    pub fn new(group_set_id: String, group_id: String) -> Self {
        SyntenyLocus {
            group_set_id,
            group_id,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyntenyBlock {
    pub block_id: String,
    pub block_size_loci: usize,
    pub group_id: String,
    pub loci: Vec<BuscoFeature>,
    pub sequence_id: String,
    pub assembly_id: String,
    pub taxon_id: String,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub length: usize,
}

impl SyntenyBlock {
    pub fn new(
        block_id: String,
        group_id: String,
        sequence_id: String,
        assembly_id: String,
        taxon_id: String,
    ) -> Self {
        SyntenyBlock {
            block_id,
            group_id,
            sequence_id,
            assembly_id,
            taxon_id,
            ..Default::default()
        }
    }

    pub fn add_locus(&mut self, locus: BuscoFeature) {
        // Update start, end and length based on the new locus
        let _start = if let Some(start) = self.start {
            Some(start.min(locus.start))
        } else {
            Some(locus.start)
        };
        let _end = if let Some(end) = self.end {
            Some(end.max(locus.end))
        } else {
            Some(locus.end)
        };
        if _end > _start {
            self.start = _start;
            self.end = _end;
        } else {
            self.start = _end;
            self.end = _start;
        }
        self.length = self.end.unwrap_or(0) - self.start.unwrap_or(0);
        self.loci.push(locus);
        self.block_size_loci += 1;
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockSetMetrics {
    pub total_loci: usize,
    pub distinct_group_count: usize,
    pub longest_block_size: usize,
    pub block_count: usize,
    pub majority_group_count: usize,
    pub majority_group_id: Option<String>,
    pub majority_group_fraction: Option<f64>,
    pub majority_group_threshold_flag: bool,
    pub filtered_transition_count_ratio: Option<f64>,
    pub filtered_gini_score: Option<f64>,
    #[deprecated(note = "Use filtered_transition_count_ratio instead")]
    pub normalised_transition_count_ratio: f64,
    #[deprecated(note = "Use filtered_gini_score instead")]
    pub normalised_gini_score: f64,
    #[deprecated(note = "This metric is no longer kept in the compact synteny summary")]
    pub normalised_minority_gini_score: f64,
    #[deprecated(note = "This metric is no longer kept in the compact synteny summary")]
    pub normalised_block_size: f64,
    #[deprecated(note = "This metric is no longer kept in the compact synteny summary")]
    pub normalised_distinct_group_count: f64,
    #[deprecated(note = "This metric is no longer kept in the compact synteny summary")]
    pub normalised_block_count: f64,
    #[deprecated(note = "This metric is no longer kept in the compact synteny summary")]
    pub normalised_interminority_transition_ratio: f64,
}

impl BlockSetMetrics {
    pub fn to_nested_attribute_docs(&self) -> Vec<NestedAttribute> {
        let mut attrs = Vec::new();
        attrs.push(NestedAttribute {
            key: "total_loci".to_string(),
            integer_value: Some(self.total_loci as i32),
            ..Default::default()
        });
        attrs.push(NestedAttribute {
            key: "distinct_group_count".to_string(),
            integer_value: Some(self.distinct_group_count as i32),
            ..Default::default()
        });
        attrs.push(NestedAttribute {
            key: "longest_block_size".to_string(),
            integer_value: Some(self.longest_block_size as i32),
            ..Default::default()
        });
        attrs.push(NestedAttribute {
            key: "block_count".to_string(),
            integer_value: Some(self.block_count as i32),
            ..Default::default()
        });
        attrs.push(NestedAttribute {
            key: "majority_group_count".to_string(),
            integer_value: Some(self.majority_group_count as i32),
            ..Default::default()
        });
        if let Some(group_id) = &self.majority_group_id {
            attrs.push(NestedAttribute {
                key: "majority_group_id".to_string(),
                keyword_value: Some(crate::parse::genomehubs::StringOrVec::Single(
                    group_id.clone(),
                )),
                ..Default::default()
            });
        }
        if let Some(value) = self.majority_group_fraction {
            attrs.push(NestedAttribute {
                key: "majority_group_fraction".to_string(),
                float_value: Some(value as f32),
                ..Default::default()
            });
        }
        attrs.push(NestedAttribute {
            key: "majority_group_threshold_flag".to_string(),
            bool_value: Some(self.majority_group_threshold_flag),
            ..Default::default()
        });
        if let Some(value) = self.filtered_transition_count_ratio {
            attrs.push(NestedAttribute {
                key: "filtered_transition_count_ratio".to_string(),
                float_value: Some(value as f32),
                ..Default::default()
            });
        }
        if let Some(value) = self.filtered_gini_score {
            attrs.push(NestedAttribute {
                key: "filtered_gini_score".to_string(),
                float_value: Some(value as f32),
                ..Default::default()
            });
        }

        #[allow(deprecated)]
        for (key, value, reason) in [
            (
                "normalised_gini_score".to_string(),
                self.normalised_gini_score,
                "Use filtered_gini_score instead",
            ),
            (
                "normalised_transition_count_ratio".to_string(),
                self.normalised_transition_count_ratio,
                "Use filtered_transition_count_ratio instead",
            ),
            (
                "normalised_minority_gini_score".to_string(),
                self.normalised_minority_gini_score,
                "This metric is no longer kept in the compact synteny summary",
            ),
            (
                "normalised_block_size".to_string(),
                self.normalised_block_size,
                "This metric is no longer kept in the compact synteny summary",
            ),
            (
                "normalised_distinct_group_count".to_string(),
                self.normalised_distinct_group_count,
                "This metric is no longer kept in the compact synteny summary",
            ),
            (
                "normalised_block_count".to_string(),
                self.normalised_block_count,
                "This metric is no longer kept in the compact synteny summary",
            ),
            (
                "normalised_interminority_transition_ratio".to_string(),
                self.normalised_interminority_transition_ratio,
                "This metric is no longer kept in the compact synteny summary",
            ),
        ] {
            attrs.push(NestedAttribute {
                key,
                float_value: Some(value as f32),
                deprecated: Some(true),
                deprecated_reason: Some(reason.to_string()),
                ..Default::default()
            });
        }

        attrs
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyntenyBlockSet {
    pub group_set_id: String,
    pub sequence_id: String,
    pub assembly_id: String,
    pub taxon_id: String,
    pub blocks: Vec<SyntenyBlock>,
    pub counts: HashMap<String, usize>, // group_id -> count of loci
    pub total_loci: usize,
    pub distinct_group_count: usize,
    pub longest_block_size: usize,
    pub latest_group_id: Option<String>,
    pub loci: Vec<SyntenyLocus>,
    pub metrics: Option<BlockSetMetrics>,
    pub transitions: Option<Vec<(String, usize)>>, // (from_group_id, to_group_id)
}

impl SyntenyBlockSet {
    pub fn new(
        group_set_id: String,
        sequence_id: String,
        assembly_id: String,
        taxon_id: String,
    ) -> Self {
        SyntenyBlockSet {
            group_set_id,
            sequence_id,
            assembly_id,
            taxon_id,
            ..Default::default()
        }
    }

    pub fn add_block(&mut self, block: SyntenyBlock) {
        self.total_loci += block.block_size_loci;
        self.counts
            .entry(block.group_id.clone())
            .and_modify(|c| *c += block.block_size_loci)
            .or_insert(block.block_size_loci);
        self.latest_group_id = Some(block.group_id.clone());
        self.longest_block_size = self.longest_block_size.max(block.block_size_loci);
        self.blocks.push(block);
        self.distinct_group_count = self.counts.len();
    }

    pub fn add_locus_to_block(&mut self, group_id: &str, locus: BuscoFeature) {
        if group_id != self.latest_group_id.as_deref().unwrap_or("") {
            // New group, create a new block
            let mut new_block = SyntenyBlock::new(
                format!("{}_block_{}", group_id, self.blocks.len() + 1),
                group_id.to_string(),
                self.sequence_id.clone(),
                self.assembly_id.clone(),
                self.taxon_id.clone(),
            );
            new_block.add_locus(locus);
            self.add_block(new_block);
        } else {
            // Add to the latest block
            if let Some(latest_block) = self.blocks.last_mut() {
                if latest_block.sequence_id.is_empty() {
                    latest_block.sequence_id = locus.sequence.clone();
                }
                latest_block.add_locus(locus);
                self.longest_block_size = self.longest_block_size.max(latest_block.block_size_loci);
                self.total_loci += 1;
                self.counts
                    .entry(group_id.to_string())
                    .and_modify(|c| *c += 1)
                    .or_insert(1);
            }
        }
    }

    pub fn normalised_gini_score(&self, group_count: usize) -> f64 {
        let n = self.total_loci as f64;
        if n == 0.0 {
            return 0.0;
        }

        let mut sum_of_squares = 0.0;
        for count in self.counts.values() {
            let p_i = *count as f64 / n;
            sum_of_squares += p_i * p_i;
        }

        let gini = 1.0 - sum_of_squares;
        let max_gini = 1.0 - (1.0 / group_count as f64);
        if max_gini == 0.0 {
            0.0
        } else {
            gini / max_gini
        }
    }

    pub fn majority_group_names(&self) -> Vec<String> {
        let max_count = self.counts.values().cloned().max().unwrap_or(0);
        self.counts
            .iter()
            .filter(|&(_, &count)| count == max_count)
            .map(|(group_id, _)| group_id.clone())
            .collect()
    }

    pub fn minority_group_names(&self) -> Vec<String> {
        let max_count = self.counts.values().cloned().max().unwrap_or(0);
        self.counts
            .iter()
            .filter(|&(_, &count)| count < max_count)
            .map(|(group_id, _)| group_id.clone())
            .collect()
    }

    pub fn normalised_minority_gini_score(&self, group_count: usize) -> f64 {
        let n = self.total_loci as f64;

        if n == 0.0 {
            return 0.0;
        }

        // filter out groups with the maximum count to focus on minority groups
        let minority_groups = self.minority_group_names();
        let minority_counts = self
            .counts
            .iter()
            .filter(|(group_id, _)| minority_groups.contains(group_id))
            .map(|(_, &count)| count)
            .collect::<Vec<usize>>();

        if minority_counts.len() < 2 {
            return 0.0; // Not enough minority groups to calculate Gini
        }

        let n = minority_counts.iter().sum::<usize>() as f64;
        if n == 0.0 {
            return 0.0;
        }

        let mut sum_of_squares = 0.0;
        for count in minority_counts.iter() {
            let p_i = *count as f64 / n;
            sum_of_squares += p_i * p_i;
        }

        let gini = 1.0 - sum_of_squares;

        let minority_group_count = group_count - (self.counts.len() - minority_counts.len());

        // Calculate the maximum possible Gini score for the given number of groups
        let max_gini = if minority_group_count > 1 {
            1.0 - (1.0 / minority_group_count as f64)
        } else {
            0.0
        };

        if max_gini == 0.0 {
            0.0
        } else {
            gini / max_gini
        }
    }

    pub fn filtered_transition_count_ratio(&self) -> Option<f64> {
        let persistent_blocks: Vec<&SyntenyBlock> = self
            .blocks
            .iter()
            .filter(|block| block.block_size_loci > 1)
            .collect();
        if persistent_blocks.len() <= 1 {
            return None;
        }

        let persistent_loci: usize = persistent_blocks
            .iter()
            .map(|block| block.block_size_loci)
            .sum();
        if persistent_loci <= 1 {
            return None;
        }

        let transitions = persistent_blocks.len() - 1;
        Some(transitions as f64 / (persistent_loci as f64 - 1.0))
    }

    pub fn normalised_transition_count_ratio(&self) -> f64 {
        if self.blocks.len() <= 1 {
            return 0.0;
        }
        let transitions = self.blocks.len() - 1;
        let ratio = transitions as f64 / (self.total_loci as f64 - 1.0);
        ratio
    }

    pub fn filtered_gini_score(&self) -> Option<f64> {
        let filtered_counts: Vec<usize> = self
            .counts
            .values()
            .copied()
            .filter(|count| *count > 1)
            .collect();
        let total = filtered_counts.iter().sum::<usize>();
        if filtered_counts.is_empty() || total == 0 {
            return None;
        }

        let mut sum_of_squares = 0.0;
        for count in &filtered_counts {
            let p_i = *count as f64 / total as f64;
            sum_of_squares += p_i * p_i;
        }

        Some(1.0 - sum_of_squares)
    }

    pub fn normalised_block_size(&self) -> f64 {
        if self.blocks.is_empty() {
            return 0.0;
        }
        let average_block_size = self.total_loci as f64 / self.blocks.len() as f64;
        let max_block_size = self.longest_block_size as f64;
        if max_block_size == 0.0 {
            0.0
        } else {
            average_block_size / max_block_size
        }
    }

    pub fn normalised_distinct_group_count(&self, group_count: usize) -> f64 {
        if group_count == 0 {
            return 0.0;
        }
        self.distinct_group_count as f64 / group_count as f64
    }

    pub fn normalised_block_count(&self, group_count: usize) -> f64 {
        if group_count == 0 {
            return 0.0;
        }
        self.blocks.len() as f64 / group_count as f64
    }

    pub fn normalised_interminority_transition_ratio(&self) -> f64 {
        if self.blocks.len() <= 1 {
            return 0.0;
        }

        let mut interminority_transitions = 0;
        let minority_groups = self.minority_group_names();
        let mut minority_block_count = 0;
        for i in 1..self.blocks.len() {
            let prev_group = &self.blocks[i - 1].group_id;
            let curr_group = &self.blocks[i].group_id;

            if prev_group != curr_group
                && minority_groups.contains(prev_group)
                && minority_groups.contains(curr_group)
            {
                interminority_transitions += 1;
            }

            if curr_group != prev_group && minority_groups.contains(curr_group) {
                minority_block_count += 1;
            }
        }
        let ratio = interminority_transitions as f64 / (minority_block_count as f64 - 1.0);
        ratio
    }

    // pub fn calculate_synteny_metrics(&self, group_count: usize) -> HashMap<String, f64> {
    //     let mut metrics = HashMap::new();
    //     metrics.insert("total_loci".to_string(), self.total_loci as f64);
    //     metrics.insert(
    //         "distinct_group_count".to_string(),
    //         self.distinct_group_count as f64,
    //     );
    //     metrics.insert(
    //         "longest_block_size".to_string(),
    //         self.longest_block_size as f64,
    //     );
    //     metrics.insert("block_count".to_string(), self.blocks.len() as f64);
    //     metrics.insert(
    //         "majority_group_count".to_string(),
    //         self.counts.values().cloned().max().unwrap_or(0) as f64,
    //     );
    //     metrics.insert(
    //         "normalised_gini_score".to_string(),
    //         self.normalised_gini_score(group_count),
    //     );
    //     metrics.insert(
    //         "normalised_minority_gini_score".to_string(),
    //         self.normalised_minority_gini_score(group_count),
    //     );
    //     metrics.insert(
    //         "normalised_transition_count_ratio".to_string(),
    //         self.normalised_transition_count_ratio(),
    //     );
    //     metrics.insert(
    //         "normalised_block_size".to_string(),
    //         self.normalised_block_size(),
    //     );
    //     metrics.insert(
    //         "normalised_distinct_group_count".to_string(),
    //         self.normalised_distinct_group_count(group_count),
    //     );
    //     metrics.insert(
    //         "normalised_block_count".to_string(),
    //         self.normalised_block_count(group_count),
    //     );
    //     metrics.insert(
    //         "normalised_interminority_transition_ratio".to_string(),
    //         self.normalised_interminority_transition_ratio(),
    //     );
    //     metrics
    // }

    pub fn set_metrics(&mut self, group_count: usize) -> () {
        let majority_group = self.counts.iter().max_by(|(_, a), (_, b)| a.cmp(b));
        let majority_group_count = majority_group.map(|(_, count)| *count).unwrap_or(0);
        let majority_group_id = majority_group.map(|(group_id, _)| group_id.clone());
        let majority_group_fraction = if self.total_loci == 0 {
            None
        } else {
            Some(majority_group_count as f64 / self.total_loci as f64)
        };
        let majority_group_threshold_flag =
            majority_group_fraction.map_or(false, |value| value > (1.0 / 3.0));

        #[allow(deprecated)]
        let metrics = BlockSetMetrics {
            total_loci: self.total_loci,
            distinct_group_count: self.distinct_group_count,
            longest_block_size: self.longest_block_size,
            block_count: self.blocks.len(),
            majority_group_count,
            majority_group_id,
            majority_group_fraction,
            majority_group_threshold_flag,
            filtered_transition_count_ratio: self.filtered_transition_count_ratio(),
            filtered_gini_score: self.filtered_gini_score(),
            normalised_transition_count_ratio: self.normalised_transition_count_ratio(),
            normalised_gini_score: self.normalised_gini_score(group_count),
            normalised_minority_gini_score: self.normalised_minority_gini_score(group_count),
            normalised_block_size: self.normalised_block_size(),
            normalised_distinct_group_count: self.normalised_distinct_group_count(group_count),
            normalised_block_count: self.normalised_block_count(group_count),
            normalised_interminority_transition_ratio: self
                .normalised_interminority_transition_ratio(),
        };
        self.metrics = Some(metrics);
    }

    pub fn get_metrics(&self) -> Option<&BlockSetMetrics> {
        self.metrics.as_ref()
    }

    pub fn set_synteny_loci(&mut self) -> () {
        // keep track of transitions between different group ids
        let mut transitions = HashMap::new();
        // Make a rank ordered list of all block sizes, and create a hashmap assigning each block id to a rank
        let mut block_sizes: Vec<(String, usize)> = self
            .blocks
            .iter()
            .map(|block| (block.block_id.clone(), block.block_size_loci))
            .collect();
        block_sizes.sort_by(|a, b| b.1.cmp(&a.1));
        let block_ranks: HashMap<String, usize> = block_sizes
            .iter()
            .enumerate()
            .map(|(rank, (block_id, _))| (block_id.clone(), rank + 1))
            .collect();

        // loop through all block sets and their blocks, and for each locus, create a SyntenyLocus struct with the appropriate metrics
        let mut loci = Vec::new();
        // get basic stats on first pass, update loci with SyntenyLocus on second pass
        let mut block_index = 0;
        let transition_threshold = 3; // configurable threshold for counting transitions
        for block in &self.blocks {
            let block_size = block.block_size_loci;
            if block_size > transition_threshold && block_index > 0 {
                let prev_block = &self.blocks[block_index - 1];
                if prev_block.block_size_loci > transition_threshold
                    && prev_block.group_id != block.group_id
                {
                    // sort group_ids alphabetically to avoid duplicate entries for the same transition in reverse order
                    let transition_string = if prev_block.group_id < block.group_id {
                        format!(
                            "{}->{}",
                            prev_block.group_id.clone(),
                            block.group_id.clone()
                        )
                    } else {
                        format!(
                            "{}->{}",
                            block.group_id.clone(),
                            prev_block.group_id.clone()
                        )
                    };
                    *transitions.entry(transition_string).or_insert(0) += 1;
                }
            }
            for (i, locus) in block.loci.iter().enumerate() {
                let rank_within_block = i + 1;
                let rank_proportion = if block_size > 0 {
                    (rank_within_block - 1) as f64 / (block_size - 1) as f64
                } else {
                    0.0
                };
                let distance_to_edge =
                    std::cmp::min(rank_within_block - 1, block_size - rank_within_block);
                let same_group_continuous = block_size;
                let mut different_group_continuous = 0;
                if rank_within_block == 1 && block_index > 0 {
                    // loop through previous blocks until we find a same group
                    let mut prev_index = block_index - 1;
                    while self.blocks[prev_index].group_id != block.group_id {
                        different_group_continuous += self.blocks[prev_index].block_size_loci;
                        if prev_index == 0 {
                            break;
                        }
                        prev_index -= 1;
                    }
                }
                if rank_within_block == block_size && block_index < self.blocks.len() - 1 {
                    // loop through next blocks until we find a same group
                    let mut next_index = block_index + 1;
                    while next_index < self.blocks.len()
                        && self.blocks[next_index].group_id != block.group_id
                    {
                        different_group_continuous += self.blocks[next_index].block_size_loci;
                        next_index += 1;
                    }
                }
                let same_group_total = block_size;
                let different_group_total =
                    self.total_loci - self.counts.get(&block.group_id).unwrap_or(&0);
                let same_to_different_ratio = if different_group_total > 0 {
                    same_group_total as f64 / different_group_total as f64
                } else {
                    0.0
                };
                let distinct_different_group_count = self.counts.len() - 1;
                let mut adjacent_group_ids = Vec::new();
                if block_index > 0 {
                    adjacent_group_ids.push(self.blocks[block_index - 1].group_id.clone());
                }
                if block_index < self.blocks.len() - 1 {
                    adjacent_group_ids.push(self.blocks[block_index + 1].group_id.clone());
                }

                loci.push(SyntenyLocus {
                    id: locus.id.clone(),
                    status: locus.status.clone(),
                    score: locus.score,
                    sequence_id: self.sequence_id.clone(),
                    assembly_id: self.assembly_id.clone(),
                    taxon_id: self.taxon_id.clone(),
                    start: locus.start,
                    end: locus.end,
                    strand: locus.strand,
                    length: locus.length,
                    group_set_id: self.group_set_id.clone(),
                    group_id: block.group_id.clone(),
                    is_primary: true,
                    block_id: block.block_id.clone(),
                    block_size_loci: block_size,
                    block_size_proportion: block_size as f64 / self.total_loci as f64,
                    block_size_rank: *block_ranks.get(&block.block_id).unwrap_or(&1),
                    rank_within_block,
                    rank_proportion,
                    distance_to_edge,
                    same_group_continuous,
                    different_group_continuous,
                    same_group_total,
                    different_group_total,
                    same_to_different_ratio,
                    distinct_different_group_count,
                    adjacent_group_ids,
                });
            }
            block_index += 1;
        }
        self.transitions = Some(transitions.into_iter().collect());
        self.loci = loci;
    }
}

fn finalize_synteny_outputs(
    busco_file: &BuscoFileConfig,
    sequence_features: &HashMap<String, FeatureDocument>,
    busco_docs: &mut HashMap<String, FeatureDocument>,
    block_sets: &mut HashMap<String, SyntenyBlockSet>,
    attribute_collector: &mut AttributeCollector,
    mode: &SyntenyIndexMode,
) -> SyntenyIndexArtifacts {
    let mut artifacts = SyntenyIndexArtifacts::default();

    if !mode.enrich_busco_features && !mode.index_synteny_loci && !mode.index_synteny_blocks {
        artifacts.busco_docs = busco_docs.drain().map(|(_, doc)| doc).collect();
        artifacts.attribute_docs = attribute_collector.take_docs();
        return artifacts;
    }

    let mut synteny_loci_by_doc_key: HashMap<String, Vec<SyntenyLocus>> = HashMap::new();
    for block_set in block_sets.values_mut() {
        block_set.set_synteny_loci();
        for locus in &block_set.loci {
            synteny_loci_by_doc_key
                .entry(feature_doc_key(&locus.id, &locus.sequence_id))
                .or_insert_with(Vec::new)
                .push(locus.clone());
        }
    }

    if mode.enrich_busco_features {
        for (doc_key, feature_doc) in busco_docs.iter_mut() {
            if let Some(synteny_loci) = synteny_loci_by_doc_key.get(doc_key) {
                let feature_attributes = ensure_feature_attributes(feature_doc);
                for locus in synteny_loci {
                    for (attr, overrides) in synteny_locus_attributes(locus) {
                        attribute_collector.add(feature_attributes, attr, overrides);
                    }
                }
            }
        }
    }

    if mode.index_synteny_loci {
        for loci in synteny_loci_by_doc_key.values() {
            for locus in loci {
                let sequence_length = sequence_features
                    .get(&locus.sequence_id)
                    .map(|sequence| sequence.sequence_length)
                    .unwrap_or(locus.length);
                let mut feature_document =
                    synteny_locus_feature_document(locus, busco_file, sequence_length);
                let feature_attributes = ensure_feature_attributes(&mut feature_document);
                for (attr, overrides) in synteny_locus_attributes(locus) {
                    attribute_collector.add(feature_attributes, attr, overrides);
                }
                artifacts.synteny_locus_docs.push(feature_document);
            }
        }
    }

    if mode.index_synteny_blocks {
        for block_set in block_sets.values() {
            for block in &block_set.blocks {
                let sequence_length = sequence_features
                    .get(&block.sequence_id)
                    .map(|sequence| sequence.sequence_length)
                    .unwrap_or(block.length.max(1));
                let mut feature_document =
                    synteny_block_feature_document(block, busco_file, sequence_length);
                let feature_attributes = ensure_feature_attributes(&mut feature_document);
                for (attr, overrides) in synteny_block_attributes(block) {
                    attribute_collector.add(feature_attributes, attr, overrides);
                }
                artifacts.synteny_block_docs.push(feature_document);
            }
        }
    }

    artifacts.busco_docs = busco_docs.drain().map(|(_, doc)| doc).collect();
    artifacts.attribute_docs = attribute_collector.take_docs();
    artifacts
}

pub fn parse_busco_files(
    busco_config: &MultiBuscoConfig,
    sequence_features: &HashMap<String, FeatureDocument>,
    window_cfg: Vec<WindowSpec>,
    lines_per_unit: usize,
    state: &mut ImportState,
    es_cfg: &EsConfig,
    import_opts: &Option<ImportOptions>,
    synteny_index_mode: Option<&SyntenyIndexMode>,
) -> Result<(), error::Error> {
    let client = ElasticsearchClient::try_from(es_cfg)?;
    let synteny_index_mode = synteny_index_mode.cloned().unwrap_or_default();
    let assembly_id = busco_config.accession.clone();
    let taxon_id = busco_config.taxon_id.clone();
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

        // set up a counter to keep track of how many shared

        let mut busco_docs: HashMap<String, FeatureDocument> = HashMap::new();
        let mut attribute_collector = AttributeCollector::new();
        let mut block_sets: HashMap<String, SyntenyBlockSet> = HashMap::new();

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
                        let block_set =
                            block_sets.entry(sequence_id.clone()).or_insert_with(|| {
                                SyntenyBlockSet::new(
                                    busco_file.lineage.clone(),
                                    sequence_id.clone(),
                                    assembly_id.clone(),
                                    taxon_id.clone(),
                                )
                            });
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

                        let feature_attributes = ensure_feature_attributes(&mut feature_document);
                        for (attr, overrides) in
                            busco_core_attributes(&f, busco_config, &sequence_id)
                        {
                            attribute_collector.add(feature_attributes, attr, overrides);
                        }

                        // Apply ALG mappings
                        let mut first_alg = true;
                        for alg in matching_algs.iter() {
                            if let Some(mapping) = &alg.mapping {
                                if let Some(mapped_id) = mapping.get(&f.id) {
                                    let (attr, overrides) =
                                        busco_alg_attribute(&alg.name, mapped_id);
                                    attribute_collector.add(feature_attributes, attr, overrides);
                                    if first_alg {
                                        block_set.add_locus_to_block(mapped_id, f.clone());
                                    }
                                }
                            }
                            first_alg = false;
                        }

                        busco_docs.insert(feature_doc_key(&f.id, &sequence_id), feature_document);
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

        // collect a hashmap of blockset metrics to add to state
        let mut blockset_metrics: HashMap<String, BlockSetMetrics> = HashMap::new();
        let group_count = matching_algs.first().and_then(|alg| alg.alg_count);
        if let Some(count) = group_count {
            for (seq_id, block_set) in &mut block_sets {
                block_set.set_metrics(count);
                if let Some(metrics) = block_set.get_metrics() {
                    blockset_metrics.insert(seq_id.clone(), metrics.clone());
                }
            }
            state.synteny_metrics_by_seq = blockset_metrics;
        }

        // Finalize synteny outputs and index documents
        let SyntenyIndexArtifacts {
            busco_docs,
            synteny_locus_docs,
            synteny_block_docs,
            attribute_docs,
        } = finalize_synteny_outputs(
            busco_file,
            sequence_features,
            &mut busco_docs,
            &mut block_sets,
            &mut attribute_collector,
            &synteny_index_mode,
        );

        if synteny_index_mode.index_synteny_loci && !synteny_locus_docs.is_empty() {
            eprintln!(
                "    Prepared {} synteny locus docs",
                synteny_locus_docs.len()
            );
        }

        if synteny_index_mode.index_synteny_blocks && !synteny_block_docs.is_empty() {
            eprintln!(
                "    Prepared {} synteny block docs",
                synteny_block_docs.len()
            );
        }

        // Index BUSCO feature documents after each file completes
        if !busco_docs.is_empty() {
            eprintln!("    Indexing {} BUSCO features", busco_docs.len());
            let wrapped_docs = client.wrap_for_bulk_index(busco_docs)?;
            client.index_documents("feature", wrapped_docs)?;
            client.refresh("feature")?;
        }

        if synteny_index_mode.index_synteny_loci && !synteny_locus_docs.is_empty() {
            eprintln!("    Indexing {} synteny loci", synteny_locus_docs.len());
            let wrapped_docs = client.wrap_for_bulk_index(synteny_locus_docs)?;
            client.index_documents("feature", wrapped_docs)?;
            client.refresh("feature")?;
        }

        if synteny_index_mode.index_synteny_blocks && !synteny_block_docs.is_empty() {
            eprintln!("    Indexing {} synteny blocks", synteny_block_docs.len());
            let wrapped_docs = client.wrap_for_bulk_index(synteny_block_docs)?;
            client.index_documents("feature", wrapped_docs)?;
            client.refresh("feature")?;
        }

        if !attribute_docs.is_empty() {
            sync_attribute_documents(attribute_docs, state, es_cfg, import_opts)?;
        }
    }
    Ok(())
}
