use std::collections::HashMap;

use crate::index::es::mappings::common::{
    keyword_property, keyword_with_fields_property, keyword_with_lookup_property,
    keyword_with_null_value_property, nested_attribute_properties, nested_identifier_properties,
    nested_property, numeric_property, text_property, Mappings, Normalizer, Property,
};

// assembly index properties
pub fn assembly_index_properties() -> HashMap<String, Property> {
    HashMap::from([
        (
            "assembly_id".to_string(),
            keyword_with_lookup_property(
                "Unique assembly ID",
                Some(32),
                Some(Normalizer::Lowercase),
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
            "parent".to_string(),
            keyword_property(
                "Taxon ID of parent taxon",
                Some(32),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "organelle".to_string(),
            keyword_with_null_value_property(
                "Primarily nucleus, mitochondrion or plastid",
                Some(16),
                Some(Normalizer::Lowercase),
                Some("nucleus".to_string()),
            ),
        ),
        (
            "attributes".to_string(),
            nested_property(nested_attribute_properties(true)),
        ),
        (
            "identifiers".to_string(),
            nested_property(nested_identifier_properties()),
        ),
    ])
}

// Set of mappings for values in the assembly index
pub fn assembly_index_mappings() -> Mappings {
    Mappings {
        dynamic: Some(false),
        properties: assembly_index_properties(),
    }
}
