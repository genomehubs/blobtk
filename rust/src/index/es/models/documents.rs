//! Define documents, which are structured representations of data for indexing and searching in Elasticsearch.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{
    index::es::models::{
        nested_documents::{NestedAttribute, NestedIdentifier},
        EsError, IndexDocument, IndexGroup,
    },
    parse::genomehubs::{
        StringOrVec, SummaryFunction, SummaryFunctionOrVec, TraverseDirection, ValueMetadataConfig,
    },
    validation::spec::{default_field_type, FieldType},
};

// The `FeatureDocument` struct represents a structured representation of a feature for indexing and searching features in Elasticsearch.
// Its structure matches the properties defined in the `feature_index_properties` function, which defines the mapping for the feature index in Elasticsearch.
// Fields share the same restrictions as the mapping to ensure compatibility with the Elasticsearch index.
#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttributeDocument {
    pub group: IndexGroup,
    pub name: String,
    // Sequence is used to order attributes in the UI, and must be a positive integer. Attributes with the same sequence will be ordered alphabetically by name.
    pub sequence: u32,
    // Field type
    #[serde(rename = "type", default = "default_field_type")]
    pub field_type: FieldType,
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
        format!("{}:{}", self.group, self.name)
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
