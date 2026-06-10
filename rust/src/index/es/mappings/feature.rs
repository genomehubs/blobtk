use std::collections::HashMap;

use crate::index::es::mappings::common::{
    keyword_property, keyword_with_fields_property, keyword_with_lookup_property,
    nested_attribute_properties, nested_identifier_properties, nested_property, numeric_property,
    text_property, Mappings, Normalizer, Property,
};

// feature index properties
pub fn feature_index_properties() -> HashMap<String, Property> {
    HashMap::from([
            (
                "feature_id".to_string(),
                keyword_with_lookup_property(
                    "Feature identifier",
                    Some(128),
                    Some(Normalizer::Lowercase),
                ),
            ),
            (
                "parent_feature_id".to_string(),
                keyword_property(
                    "Parent feature ID (if applicable)",
                    Some(128),
                    Some(Normalizer::Lowercase),
                ),
            ),
            (
                "primary_type".to_string(),
                keyword_property(
                    "Primary type of the feature",
                    Some(64),
                    Some(Normalizer::Lowercase),
                ),
            ),
            (
                "assembly_id".to_string(),
                keyword_with_fields_property(
                    "Unique assembly ID",
                    Some(32),
                    Some(Normalizer::Lowercase),
                    HashMap::from([(
                        "text".to_string(),
                        text_property("Text field for assembly ID", None, None),
                    )]),
                ),
            ),
            (
                "taxon_id".to_string(),
                keyword_property(
                    "Taxonomy-specific taxon ID",
                    Some(32),
                    Some(Normalizer::Lowercase),
                ),
            ),
            (
                "file_id".to_string(),
                keyword_property(
                    "Unique file ID",
                    Some(64),
                    Some(Normalizer::Lowercase),
                ),
            ),
            (
                "analysis_id".to_string(),
                keyword_property(
                    "Unique analysis ID",
                    Some(64),
                    Some(Normalizer::Lowercase),
                ),
            ),
            (
                "ancestors".to_string(),
                keyword_property(
                    "Taxon IDs of ancestral taxa",
                    Some(32),
                    Some(Normalizer::Lowercase),
                ),
            ),
            (
                "sequence_id".to_string(),
                keyword_property(
                    "Sequence ID of feature coordinates",
                    Some(64),
                    Some(Normalizer::Lowercase),
                ),
            ),
            (
                "start".to_string(),
                numeric_property(
                    "Start coordinate of the feature",
                    "long",
                    None,
                ),
            ),
            (
                "end".to_string(),
                numeric_property(
                    "End coordinate of the feature",
                    "long",
                    None,
                ),
            ),
            (
                "length".to_string(),
                numeric_property(
                    "Length of the feature",
                    "long",
                    None,
                ),
            ),
            (
                "strand".to_string(),
                numeric_property(
                    "Strand of the feature (1 for forward, -1 for reverse, 0 for unknown)",
                    "byte",
                    None,
                ),
            ),
            (
                "sequence_length".to_string(),
                numeric_property(
                    "Length of the parent sequence",
                    "long",
                    None,
                ),
            ),
            (
                "container_ids".to_string(),
                 keyword_property(
                    "IDs of features that overlap this feature at different resolutions (e.g. win_1m:…, win_100k:…)",
                    Some(128),
                    Some(Normalizer::Lowercase),
                ),
            ),
            (
                "attributes".to_string(),
                nested_property(
                    nested_attribute_properties(true),
                ),
            ),
            (
                "identifiers".to_string(),
                nested_property(
                    nested_identifier_properties(),
                ),
            ),
        ])
}

// Set of mappings for values in the feature index
pub fn feature_index_mappings() -> Mappings {
    Mappings {
        dynamic: Some(false),
        properties: feature_index_properties(),
    }
}
