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
            vec![format!("{}_unknown", lineage)]
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyntenyLocus {
    pub id: String,
    pub status: String,
    pub score: f64,
    pub sequence_id: String,
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
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub length: usize,
}

impl SyntenyBlock {
    pub fn new(block_id: String, group_id: String) -> Self {
        SyntenyBlock {
            block_id,
            group_id,
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
    pub normalised_gini_score: f64,
    pub normalised_minority_gini_score: f64,
    pub normalised_transition_count_ratio: f64,
    pub normalised_block_size: f64,
    pub normalised_distinct_group_count: f64,
    pub normalised_block_count: f64,
    pub normalised_interminority_transition_ratio: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyntenyBlockSet {
    pub group_set_id: String,
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
    pub fn new(group_set_id: String) -> Self {
        SyntenyBlockSet {
            group_set_id,
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
            );
            new_block.add_locus(locus);
            self.add_block(new_block);
        } else {
            // Add to the latest block
            if let Some(latest_block) = self.blocks.last_mut() {
                latest_block.add_locus(locus);
                self.longest_block_size = self.longest_block_size.max(latest_block.block_size_loci)
            }
        }
        self.total_loci += 1;
        self.counts
            .entry(group_id.to_string())
            .and_modify(|c| *c += 1)
            .or_insert(1);
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
        let max_count = self.counts.values().cloned().max().unwrap_or(0);
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

    pub fn normalised_transition_count_ratio(&self) -> f64 {
        if self.blocks.len() <= 1 {
            return 0.0;
        }
        let transitions = self.blocks.len() - 1;
        let ratio = transitions as f64 / (self.total_loci as f64 - 1.0);
        ratio
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
        let metrics = BlockSetMetrics {
            total_loci: self.total_loci,
            distinct_group_count: self.distinct_group_count,
            longest_block_size: self.longest_block_size,
            block_count: self.blocks.len(),
            majority_group_count: self.counts.values().cloned().max().unwrap_or(0),
            normalised_gini_score: self.normalised_gini_score(group_count),
            normalised_minority_gini_score: self.normalised_minority_gini_score(group_count),
            normalised_transition_count_ratio: self.normalised_transition_count_ratio(),
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
                    sequence_id: locus.sequence.clone(),
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
        dbg!(&self.loci);
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

        // set up a counter to keep track of how many shared

        let mut busco_docs = Vec::new();
        let mut attribute_docs = Vec::new();
        let mut seen_attributes = std::collections::HashSet::new();
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
                        let block_set = block_sets
                            .entry(sequence_id.clone())
                            .or_insert_with(|| SyntenyBlockSet::new(busco_file.lineage.clone()));
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
                        let mut first_alg = true;
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
                                    if first_alg {
                                        block_set.add_locus_to_block(mapped_id, f.clone());
                                    }
                                }
                            }
                            first_alg = false;
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

        // let metrics = block_sets
        //     .iter()
        //     .map(|(seq_id, block_set)| {
        //         let group_count = block_set.counts.len();
        //         block_set.set_metrics(group_count);
        //         let synteny_metrics = block_set.get_metrics();
        //         (seq_id.clone(), synteny_metrics)
        //     })
        //     .collect::<HashMap<String, HashMap<String, f64>>>();
        // dbg!(&metrics);

        let synteny_loci = block_sets
            .iter_mut()
            .map(|(seq_id, block_set)| {
                block_set.set_synteny_loci();
                (seq_id.clone(), block_set.loci.clone())
            })
            .collect::<HashMap<String, Vec<SyntenyLocus>>>();
        // dbg!(&synteny_loci);

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
