use std::collections::HashMap;

use crate::index::es::mappings::common::{
    keyword_property, numeric_property, object_property, text_property, Mappings, Normalizer,
    Property,
};

// Set of properties for the attribute index
pub fn attribute_index_properties() -> HashMap<String, Property> {
    HashMap::from([
        (
            "group".to_string(),
            keyword_property(
                "Index group (e.g. assembly or taxon)",
                Some(16),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "name".to_string(),
            keyword_property("Attribute name", Some(32), Some(Normalizer::Lowercase)),
        ),
        (
            "synonyms".to_string(),
            keyword_property(
                "Attribute name synonyms",
                Some(32),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "display_name".to_string(),
            keyword_property("Attribute display name", Some(32), None),
        ),
        (
            "default".to_string(),
            keyword_property("Default attribute value", Some(64), None),
        ),
        (
            "sequence".to_string(),
            numeric_property("Attribute display order", "integer", Some(0)),
        ),
        (
            "constraint".to_string(),
            object_property("Attribute constraint"),
        ),
        (
            "description".to_string(),
            text_property("Attribute description", Some(true), None),
        ),
        (
            "display_level".to_string(),
            numeric_property("Display priority", "byte", None),
        ),
        (
            "index".to_string(),
            numeric_property("Column index of value in original file", "short", None),
        ),
        (
            "separator".to_string(),
            text_property("Value separator", Some(true), None),
        ),
        (
            "summary".to_string(),
            keyword_property("Summary function(s) to apply to raw values", Some(32), None),
        ),
        (
            "translate".to_string(),
            object_property("Attribute translation"),
        ),
        (
            "traverse".to_string(),
            keyword_property("Summary function to use in tree traversal", Some(32), None),
        ),
        (
            "traverse_direction".to_string(),
            keyword_property("Restrict tree traversal direction", Some(4), None),
        ),
        (
            "traverse_up_limit".to_string(),
            keyword_property("Rank to stop upward tree traversal", Some(32), None),
        ),
        (
            "traverse_down_limit".to_string(),
            keyword_property("Rank to stop downward tree traversal", Some(32), None),
        ),
        (
            "type".to_string(),
            keyword_property("Data type", Some(32), None),
        ),
        (
            "units".to_string(),
            keyword_property("Units for values", Some(32), None),
        ),
        (
            "value_metadata".to_string(),
            object_property("value metadata"),
        ),
    ])
}

// Set of mappings for the attribute index
pub fn attribute_index_mappings() -> Mappings {
    Mappings {
        dynamic: Some(false),
        properties: attribute_index_properties(),
    }
}
