use crate::index::es::models::attribute_builder::{
    build_attribute_document, AttributeDocOverrides,
};
use crate::index::es::models::documents::AttributeDocument;
use crate::index::es::models::documents::FeatureDocument;
use crate::index::es::models::nested_documents::NestedAttribute;
use crate::parse::busco::{
    BuscoFeature, BuscoFileConfig, MultiBuscoConfig, SyntenyBlock, SyntenyLocus,
};
use crate::parse::genomehubs::StringOrVec;
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SyntenyIndexMode {
    #[serde(default = "default_true")]
    pub enrich_busco_features: bool,
    #[serde(default = "default_true")]
    pub index_synteny_loci: bool,
    #[serde(default = "default_true")]
    pub index_synteny_blocks: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Default)]
pub struct SyntenyIndexArtifacts {
    pub busco_docs: Vec<FeatureDocument>,
    pub synteny_locus_docs: Vec<FeatureDocument>,
    pub synteny_block_docs: Vec<FeatureDocument>,
    pub attribute_docs: Vec<AttributeDocument>,
}

pub struct AttributeCollector {
    seen: HashSet<String>,
    docs: Vec<AttributeDocument>,
}

impl AttributeCollector {
    pub fn new() -> Self {
        Self {
            seen: HashSet::new(),
            docs: Vec::new(),
        }
    }

    pub fn add(
        &mut self,
        feature_attrs: &mut Vec<NestedAttribute>,
        attr: NestedAttribute,
        overrides: Option<AttributeDocOverrides>,
    ) {
        let key = attr.key.clone();
        feature_attrs.push(attr.clone());

        if self.seen.insert(key) {
            self.docs
                .push(build_attribute_document(&attr, overrides.as_ref()));
        }
    }

    pub fn into_docs(self) -> Vec<AttributeDocument> {
        self.docs
    }

    pub fn take_docs(&mut self) -> Vec<AttributeDocument> {
        std::mem::take(&mut self.docs)
    }
}

pub fn busco_core_attributes(
    feature: &BuscoFeature,
    busco_config: &MultiBuscoConfig,
    sequence_id: &str,
) -> Vec<(NestedAttribute, Option<AttributeDocOverrides>)> {
    let default_overrides = AttributeDocOverrides {
        display_group: Some("busco".to_string()),
        ..Default::default()
    };
    let attributes = vec![
        (
            NestedAttribute {
                key: "busco_name".to_string(),
                keyword_value: Some(StringOrVec::Single(feature.id.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("BUSCO Name".to_string()),
                description: Some("BUSCO locus name".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "busco_score".to_string(),
                float_value: Some(feature.score as f32),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("BUSCO Score".to_string()),
                description: Some("BUSCO prediction score".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "busco_status".to_string(),
                keyword_value: Some(StringOrVec::Single(feature.status.to_lowercase())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("BUSCO Status".to_string()),
                description: Some("BUSCO gene prediction status".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "assembly_id".to_string(),
                keyword_value: Some(StringOrVec::Single(busco_config.accession.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Assembly ID".to_string()),
                description: Some("Assembly accession for the BUSCO feature".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "taxon_id".to_string(),
                keyword_value: Some(StringOrVec::Single(busco_config.taxon_id.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Taxon ID".to_string()),
                description: Some("Taxon accession for the BUSCO feature".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "sequence_id".to_string(),
                keyword_value: Some(StringOrVec::Single(sequence_id.to_string())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Sequence ID".to_string()),
                description: Some("Sequence containing the BUSCO feature".to_string()),
                ..default_overrides.clone()
            }),
        ),
    ];
    attributes
}

pub fn busco_alg_attribute(
    alg_name: &str,
    mapped_id: &str,
) -> (NestedAttribute, Option<AttributeDocOverrides>) {
    (
        NestedAttribute {
            key: alg_name.to_string(),
            keyword_value: Some(StringOrVec::Single(mapped_id.to_string())),
            ..Default::default()
        },
        Some(AttributeDocOverrides {
            display_name: Some(format!("BUSCO {}", alg_name)),
            display_group: Some("alg".to_string()),
            description: Some(format!("ALG mapping for {}", alg_name)),
            ..Default::default()
        }),
    )
}

pub fn synteny_locus_attributes(
    locus: &SyntenyLocus,
) -> Vec<(NestedAttribute, Option<AttributeDocOverrides>)> {
    let default_overrides = AttributeDocOverrides {
        display_group: Some("synteny".to_string()),
        ..Default::default()
    };
    let attributes = vec![
        // sequence_id
        (
            NestedAttribute {
                key: "sequence_id".to_string(),
                keyword_value: Some(StringOrVec::Single(locus.sequence_id.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Sequence ID".to_string()),
                description: Some("Sequence containing the synteny locus".to_string()),
                ..default_overrides.clone()
            }),
        ),
        // assembly_id
        (
            NestedAttribute {
                key: "assembly_id".to_string(),
                keyword_value: Some(StringOrVec::Single(locus.assembly_id.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Assembly ID".to_string()),
                description: Some("Assembly accession for the synteny locus".to_string()),
                ..default_overrides.clone()
            }),
        ),
        // taxon ID
        (
            NestedAttribute {
                key: "taxon_id".to_string(),
                keyword_value: Some(StringOrVec::Single(locus.taxon_id.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Taxon ID".to_string()),
                description: Some("Taxon accession for the synteny locus".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "group_set_id".to_string(),
                keyword_value: Some(StringOrVec::Single(locus.group_set_id.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Group Set ID".to_string()),
                description: Some("Group set used for synteny classification".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "group_id".to_string(),
                keyword_value: Some(StringOrVec::Single(locus.group_id.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Group ID".to_string()),
                description: Some("Group assignment for the locus".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "is_primary".to_string(),
                bool_value: Some(locus.is_primary),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Primary Group".to_string()),
                description: Some("Whether the locus belongs to the primary group set".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "block_id".to_string(),
                keyword_value: Some(StringOrVec::Single(locus.block_id.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Block ID".to_string()),
                description: Some("Contiguous synteny block identifier".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "block_size_loci".to_string(),
                long_value: Some(locus.block_size_loci as i64),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Block Size Loci".to_string()),
                description: Some("Number of BUSCO loci in the block".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "block_size_proportion".to_string(),
                double_value: Some(locus.block_size_proportion),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Block Size Proportion".to_string()),
                description: Some("Block size relative to the selected context".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "block_size_rank".to_string(),
                long_value: Some(locus.block_size_rank as i64),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Block Size Rank".to_string()),
                description: Some("Rank of the block by size".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "rank_within_block".to_string(),
                long_value: Some(locus.rank_within_block as i64),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Rank Within Block".to_string()),
                description: Some("One-based locus rank within the block".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "rank_proportion".to_string(),
                double_value: Some(locus.rank_proportion),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Rank Proportion".to_string()),
                description: Some("Normalized locus position within the block".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "distance_to_edge".to_string(),
                long_value: Some(locus.distance_to_edge as i64),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Distance To Edge".to_string()),
                description: Some("Distance in loci to the nearest block edge".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "same_group_continuous".to_string(),
                long_value: Some(locus.same_group_continuous as i64),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Same Group Continuous".to_string()),
                description: Some("Continuous same-group loci around the block".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "different_group_continuous".to_string(),
                long_value: Some(locus.different_group_continuous as i64),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Different Group Continuous".to_string()),
                description: Some("Continuous different-group loci around the block".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "same_group_total".to_string(),
                long_value: Some(locus.same_group_total as i64),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Same Group Total".to_string()),
                description: Some("Total same-group loci in the context".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "different_group_total".to_string(),
                long_value: Some(locus.different_group_total as i64),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Different Group Total".to_string()),
                description: Some("Total different-group loci in the context".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "same_to_different_ratio".to_string(),
                double_value: Some(locus.same_to_different_ratio),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Same To Different Ratio".to_string()),
                description: Some("Ratio of same-group to different-group loci".to_string()),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "distinct_different_group_count".to_string(),
                long_value: Some(locus.distinct_different_group_count as i64),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Distinct Different Group Count".to_string()),
                description: Some(
                    "Count of different groups represented in the context".to_string(),
                ),
                ..default_overrides.clone()
            }),
        ),
        (
            NestedAttribute {
                key: "adjacent_group_ids".to_string(),
                keyword_value: Some(StringOrVec::Multiple(locus.adjacent_group_ids.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Adjacent Group IDs".to_string()),
                description: Some("Groups adjacent to the primary block".to_string()),
                ..default_overrides.clone()
            }),
        ),
    ];
    attributes
}

fn synteny_feature_document(
    doc_id: String,
    doc_name: Option<String>,
    primary_type: String,
    start: usize,
    end: usize,
    strand: Option<i8>,
    sequence_id: String,
    sequence_length: usize,
    busco_file: &BuscoFileConfig,
    analysis_id: String,
) -> FeatureDocument {
    FeatureDocument::new(
        doc_id,
        doc_name,
        primary_type,
        start,
        end,
        strand,
        None,
        sequence_id,
        sequence_length,
        busco_file.accession.clone(),
        busco_file.taxon_id.clone(),
        None,
        Some(busco_file.path.to_string_lossy().to_string()),
        Some(analysis_id),
    )
}

pub fn synteny_locus_feature_document(
    locus: &SyntenyLocus,
    busco_file: &BuscoFileConfig,
    sequence_length: usize,
) -> FeatureDocument {
    synteny_feature_document(
        format!("{}::{}::locus", locus.sequence_id, locus.id),
        Some(locus.id.clone()),
        format!("{}-synteny-locus", busco_file.lineage),
        locus.start,
        locus.end,
        Some(locus.strand),
        locus.sequence_id.clone(),
        sequence_length,
        busco_file,
        "busco_synteny".to_string(),
    )
}

pub fn synteny_block_feature_document(
    block: &SyntenyBlock,
    busco_file: &BuscoFileConfig,
    sequence_length: usize,
) -> FeatureDocument {
    let start = block.start.unwrap_or(1);
    let end = block.end.unwrap_or(start);
    synteny_feature_document(
        format!("{}::{}::block", block.sequence_id, block.block_id),
        Some(block.block_id.clone()),
        format!("{}-synteny-block", busco_file.lineage),
        start,
        end,
        Some(1),
        block.sequence_id.clone(),
        sequence_length,
        busco_file,
        "busco_synteny".to_string(),
    )
}

pub fn synteny_block_attributes(
    block: &SyntenyBlock,
) -> Vec<(NestedAttribute, Option<AttributeDocOverrides>)> {
    let attributes = vec![
        // sequence_id
        (
            NestedAttribute {
                key: "sequence_id".to_string(),
                keyword_value: Some(StringOrVec::Single(block.sequence_id.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Sequence ID".to_string()),
                description: Some("Sequence containing the synteny block".to_string()),
                ..Default::default()
            }),
        ),
        // assembly_id
        (
            NestedAttribute {
                key: "assembly_id".to_string(),
                keyword_value: Some(StringOrVec::Single(block.assembly_id.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Assembly ID".to_string()),
                description: Some("Assembly accession for the synteny block".to_string()),
                ..Default::default()
            }),
        ),
        // taxon ID
        (
            NestedAttribute {
                key: "taxon_id".to_string(),
                keyword_value: Some(StringOrVec::Single(block.taxon_id.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Taxon ID".to_string()),
                description: Some("Taxon accession for the synteny block".to_string()),
                ..Default::default()
            }),
        ),
        (
            NestedAttribute {
                key: "block_id".to_string(),
                keyword_value: Some(StringOrVec::Single(block.block_id.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Block ID".to_string()),
                description: Some("Contiguous synteny block identifier".to_string()),
                ..Default::default()
            }),
        ),
        (
            NestedAttribute {
                key: "group_id".to_string(),
                keyword_value: Some(StringOrVec::Single(block.group_id.clone())),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Group ID".to_string()),
                description: Some("Group assigned to the block".to_string()),
                ..Default::default()
            }),
        ),
        (
            NestedAttribute {
                key: "block_size_loci".to_string(),
                long_value: Some(block.block_size_loci as i64),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Block Size Loci".to_string()),
                description: Some("Number of loci in the block".to_string()),
                ..Default::default()
            }),
        ),
        (
            NestedAttribute {
                key: "start".to_string(),
                long_value: block.start.map(|value| value as i64),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Start".to_string()),
                description: Some("Block start coordinate".to_string()),
                ..Default::default()
            }),
        ),
        (
            NestedAttribute {
                key: "end".to_string(),
                long_value: block.end.map(|value| value as i64),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("End".to_string()),
                description: Some("Block end coordinate".to_string()),
                ..Default::default()
            }),
        ),
        (
            NestedAttribute {
                key: "length".to_string(),
                long_value: Some(block.length as i64),
                ..Default::default()
            },
            Some(AttributeDocOverrides {
                display_name: Some("Length".to_string()),
                description: Some("Span of the block".to_string()),
                ..Default::default()
            }),
        ),
    ];
    attributes
}
