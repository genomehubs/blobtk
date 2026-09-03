//! Define documents, which are structured representations of data for indexing and searching in Elasticsearch.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{
    attribute_registry::{AttributeDestination, AttributeRegistry},
    index::es::models::{
        nested_documents::{NestedAttribute, NestedIdentifier},
        BuildDocument, EsError, IndexDocument, IndexGroup,
    },
    parse::genomehubs::{
        StringOrVec, SummaryFunction, SummaryFunctionOrVec, TraverseDirection, ValueMetadataConfig,
    },
    validation::spec::{default_field_type, FieldType},
};

pub fn feature_type_aliases(primary_type: &str) -> Vec<String> {
    let mut feature_types = vec![primary_type.to_string()];
    if primary_type.starts_with("win") {
        feature_types.push("window".to_string());
    } else if primary_type.contains("synteny-locus") {
        feature_types.push("synteny-locus".to_string());
        feature_types.push("locus".to_string());
    } else if primary_type.contains("synteny-block") {
        feature_types.push("synteny-block".to_string());
        feature_types.push("block".to_string());
    } else if primary_type.contains("_odb") {
        feature_types.push("busco-gene".to_string());
        feature_types.push("gene".to_string());
    } else if primary_type == "chromosome" || primary_type == "scaffold" || primary_type == "contig"
    {
        feature_types.push("sequence".to_string());
        feature_types.push("nuclear-sequence".to_string());
        feature_types.push("toplevel".to_string());
    } else if primary_type == "mitochondrion"
        || primary_type == "chloroplast"
        || primary_type == "apicoplast"
        || primary_type == "plastid"
    {
        feature_types.push("sequence".to_string());
        feature_types.push("organelle-sequence".to_string());
    } else {
        dbg!(&primary_type); // --- IGNORE ---
    }
    feature_types
}

// The `FeatureDocument` struct represents a structured representation of a feature for indexing and searching features in Elasticsearch.
// Its structure matches the properties defined in the `feature_index_properties` function, which defines the mapping for the feature index in Elasticsearch.
// Fields share the same restrictions as the mapping to ensure compatibility with the Elasticsearch index.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FeatureDocument {
    // the `feature_id` field is a unique identifier for the feature, which is required and must be a string with a maximum length of 128 characters. It is normalized to lowercase and indexed as a keyword for efficient searching.
    pub feature_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_feature_id: Option<String>,
    pub primary_type: String,
    pub start: usize,
    pub end: usize,
    pub length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strand: Option<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_ids: Option<Vec<String>>,
    pub sequence_id: String,
    pub sequence_length: usize,
    pub assembly_id: String,
    pub taxon_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ancestors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<NestedAttribute>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifiers: Option<Vec<NestedIdentifier>>,
}

impl FeatureDocument {
    fn canonical_top_level_field(key: &str) -> bool {
        matches!(
            key,
            "feature_id"
                | "parent_feature_id"
                | "primary_type"
                | "start"
                | "end"
                | "length"
                | "strand"
                | "container_ids"
                | "sequence_id"
                | "sequence_length"
                | "assembly_id"
                | "taxon_id"
                | "ancestors"
                | "file_id"
                | "analysis_id"
        )
    }

    pub fn new(
        feature_id: String,
        parent_feature_id: Option<String>,
        primary_type: String,
        start: usize,
        end: usize,
        //length: usize,
        strand: Option<i8>,
        container_ids: Option<Vec<String>>,
        sequence_id: String,
        sequence_length: usize,
        assembly_id: String,
        taxon_id: String,
        ancestors: Option<Vec<String>>,
        file_id: Option<String>,
        analysis_id: Option<String>,
        // attributes: Option<Vec<NestedAttribute>>,
        // identifiers: Option<Vec<NestedIdentifier>>,
    ) -> Self {
        let (start, end) = if end > start {
            (start, end)
        } else {
            (end, start)
        };
        let length = end - start;
        let seq_proportion = if sequence_length > 0 {
            (length as f64 / sequence_length as f64) as f32
        } else {
            0.0
        };
        let midpoint = start + (length / 2);
        let midpoint_proportion = if sequence_length > 0 {
            (midpoint as f64 / sequence_length as f64) as f32
        } else {
            0.0
        };
        let feature_types = feature_type_aliases(&primary_type);
        let registry = AttributeRegistry::load_default().ok();
        let should_store_nested = |key: &str| {
            registry
                .as_ref()
                .map(|registry| registry.is_nested_attribute(key))
                .unwrap_or(true)
        };
        let should_store_top_level = |key: &str| {
            registry
                .as_ref()
                .map(|registry| registry.is_top_level_attribute(key))
                .unwrap_or(false)
        };

        let mut attributes_list = vec![];
        let nested_entries = [
            (
                "start".to_string(),
                NestedAttribute {
                    key: "start".to_string(),
                    long_value: Some(start as i64),
                    ..Default::default()
                },
            ),
            (
                "end".to_string(),
                NestedAttribute {
                    key: "end".to_string(),
                    long_value: Some(end as i64),
                    ..Default::default()
                },
            ),
            (
                "length".to_string(),
                NestedAttribute {
                    key: "length".to_string(),
                    long_value: Some(length as i64),
                    ..Default::default()
                },
            ),
            (
                "seq_proportion".to_string(),
                NestedAttribute {
                    key: "seq_proportion".to_string(),
                    float_value: Some(seq_proportion),
                    ..Default::default()
                },
            ),
            (
                "midpoint".to_string(),
                NestedAttribute {
                    key: "midpoint".to_string(),
                    long_value: Some(midpoint as i64),
                    ..Default::default()
                },
            ),
            (
                "midpoint_proportion".to_string(),
                NestedAttribute {
                    key: "midpoint_proportion".to_string(),
                    float_value: Some(midpoint_proportion),
                    ..Default::default()
                },
            ),
            (
                "feature_type".to_string(),
                NestedAttribute {
                    key: "feature_type".to_string(),
                    keyword_value: Some(StringOrVec::Multiple(feature_types)),
                    ..Default::default()
                },
            ),
        ];
        for (key, attr) in nested_entries {
            if should_store_nested(&key) {
                attributes_list.push(attr.clone());
            }
            if should_store_top_level(&key) && !Self::canonical_top_level_field(&key) {
                // Only explicitly mapped top-level fields are valid. There is no fallback
                // `extra_fields` bucket.
                let _ = attr;
            }
        }
        if let Some(strand_value) = strand {
            if should_store_nested("strand") {
                attributes_list.push(NestedAttribute {
                    key: "strand".to_string(),
                    byte_value: Some(strand_value),
                    ..Default::default()
                });
            }
            if should_store_top_level("strand") && !Self::canonical_top_level_field("strand") {
                let _ = strand_value;
            }
        }
        let feature_id = if feature_id.starts_with(&sequence_id) {
            feature_id.clone()
        } else {
            let feature_id = format!("{}:{}-{}:{}", sequence_id, start, end, feature_id);
            feature_id
        };
        FeatureDocument {
            feature_id,
            parent_feature_id,
            primary_type,
            start,
            end,
            length,
            strand,
            container_ids,
            sequence_id,
            sequence_length,
            assembly_id,
            taxon_id,
            ancestors,
            file_id,
            analysis_id,
            attributes: Some(attributes_list),
            identifiers: None,
        }
    }
}

fn nested_attribute_as_value(attr: &NestedAttribute) -> Option<serde_json::Value> {
    if let Some(value) = attr.keyword_value.as_ref() {
        return Some(serde_json::to_value(value).unwrap_or(serde_json::Value::Null));
    }
    if let Some(value) = attr.bool_value {
        return Some(serde_json::Value::Bool(value));
    }
    if let Some(value) = attr.byte_value {
        return Some(serde_json::Value::Number(serde_json::Number::from(value)));
    }
    if let Some(value) = attr.short_value {
        return Some(serde_json::Value::Number(serde_json::Number::from(value)));
    }
    if let Some(value) = attr.integer_value {
        return Some(serde_json::Value::Number(serde_json::Number::from(value)));
    }
    if let Some(value) = attr.long_value {
        return Some(serde_json::Value::Number(serde_json::Number::from(value)));
    }
    if let Some(value) = attr.float_value {
        return Some(serde_json::json!(value));
    }
    if let Some(value) = attr.double_value {
        return Some(serde_json::json!(value));
    }
    if let Some(value) = attr.half_float_value {
        return Some(serde_json::json!(value));
    }
    if let Some(value) = attr.one_dp_value {
        return Some(serde_json::json!(value));
    }
    if let Some(value) = attr.two_dp_value {
        return Some(serde_json::json!(value));
    }
    if let Some(value) = attr.three_dp_value {
        return Some(serde_json::json!(value));
    }
    if let Some(value) = attr.four_dp_value {
        return Some(serde_json::json!(value));
    }
    if let Some(value) = attr.date_value.clone() {
        return Some(serde_json::Value::String(value));
    }
    if let Some(value) = attr.text_value.clone() {
        return Some(serde_json::Value::String(value));
    }
    None
}

impl BuildDocument for FeatureDocument {
    fn add_attribute(
        &mut self,
        attribute: super::nested_documents::NestedAttribute,
    ) -> Result<(), EsError> {
        let registry = AttributeRegistry::load_default().ok();
        let key = attribute.key.clone();
        let is_nested = registry
            .as_ref()
            .map(|registry| registry.is_nested_attribute(&key))
            .unwrap_or(true);
        let is_top_level = registry
            .as_ref()
            .map(|registry| registry.is_top_level_attribute(&key))
            .unwrap_or(false);

        if is_nested {
            if let Some(attrs) = &mut self.attributes {
                attrs.push(attribute.clone());
            } else {
                self.attributes = Some(vec![attribute.clone()]);
            }
        }
        if is_top_level && !Self::canonical_top_level_field(&key) {
            // Top-level destination means the attribute must be a real field in the
            // feature schema, not a bucketed fallback. Unknown or unmapped attributes
            // are rejected by the registry checks upstream.
            let _ = attribute;
        }
        Ok(())
    }
}

// implement the `IndexDocument` trait for `FeatureDocument` to allow it to be indexed in Elasticsearch
impl IndexDocument for FeatureDocument {
    fn get_id(&self) -> String {
        self.feature_id.clone()
    }

    fn index_group(&self) -> IndexGroup {
        IndexGroup::Feature
    }

    fn validate(&self) -> Result<(), EsError> {
        if self.feature_id.is_empty() {
            return Err(EsError::ValidationError(
                "feature_id cannot be empty".to_string(),
            ));
        }
        if self.primary_type.is_empty() {
            return Err(EsError::ValidationError(
                "primary_type cannot be empty".to_string(),
            ));
        }
        if self.assembly_id.is_empty() {
            return Err(EsError::ValidationError(
                "assembly_id cannot be empty".to_string(),
            ));
        }
        if self.taxon_id.is_empty() {
            return Err(EsError::ValidationError(
                "taxon_id cannot be empty".to_string(),
            ));
        }
        if self.sequence_id.is_empty() {
            return Err(EsError::ValidationError(
                "sequence_id cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AttributeDocument {
    pub group: IndexGroup,
    pub name: String,
    // Sequence is used to order attributes in the UI, and must be a positive integer. Attributes with the same sequence will be ordered alphabetically by name.
    pub sequence: u32,
    // Field type
    #[serde(rename = "type", default = "default_field_type")]
    pub field_type: FieldType,
    // Display group is used to group attributes in the UI, and must be a string. Attributes with the same display group will be displayed together in the UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synonyms: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraint: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_level: Option<u8>,
    // Column index of value in original file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u16>,
    // Value separator for multi-valued attributes in original file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<SummaryFunctionOrVec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translate: Option<HashMap<String, StringOrVec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traverse: Option<SummaryFunction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traverse_direction: Option<TraverseDirection>,
    // Traverse limit is a taxon rank at which to stop filling values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traverse_up_limit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traverse_down_limit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_metadata: Option<HashMap<String, ValueMetadataConfig>>,
}

impl IndexDocument for AttributeDocument {
    fn get_id(&self) -> String {
        self.name.clone()
    }

    fn index_group(&self) -> IndexGroup {
        self.group.clone()
    }

    fn validate(&self) -> Result<(), EsError> {
        if self.name.is_empty() {
            return Err(EsError::ValidationError(
                "Attribute name cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{feature_type_aliases, FeatureDocument};

    #[test]
    fn feature_type_aliases_include_sequence_flags_for_sequence_documents() {
        let aliases = feature_type_aliases("chromosome");
        assert!(aliases.contains(&"chromosome".to_string()));
        assert!(aliases.contains(&"sequence".to_string()));
        assert!(aliases.contains(&"nuclear-sequence".to_string()));
        assert!(aliases.contains(&"toplevel".to_string()));
    }

    #[test]
    fn feature_type_aliases_include_window_flags_for_window_documents() {
        let aliases = feature_type_aliases("win-2k");
        assert!(aliases.contains(&"win-2k".to_string()));
        assert!(aliases.contains(&"window".to_string()));
    }

    #[test]
    fn canonical_feature_fields_stay_top_level_and_do_not_duplicate_in_extra_fields() {
        let doc = FeatureDocument::new(
            "feat1".to_string(),
            None,
            "gene".to_string(),
            100,
            500,
            Some(1),
            None,
            "chr1".to_string(),
            1000,
            "asm1".to_string(),
            "9606".to_string(),
            None,
            None,
            None,
        );

        assert_eq!(doc.start, 100);
        assert_eq!(doc.end, 500);
        assert_eq!(doc.length, 400);
        assert_eq!(doc.sequence_id, "chr1");
        assert_eq!(doc.assembly_id, "asm1");
        assert_eq!(doc.taxon_id, "9606");
    }
}
