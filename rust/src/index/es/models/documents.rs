//! Define documents, which are structured representations of data for indexing and searching in Elasticsearch.

use serde::{Deserialize, Serialize};

use crate::index::es::models::{EsError, IndexDocument};

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
    pub assembly_id: String,
    pub taxon_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ancestors: Option<Vec<String>>,
    pub sequence_id: String,
    pub start: usize,
    pub end: usize,
    pub length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strand: Option<i8>,
    pub sequence_length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifiers: Option<Vec<serde_json::Value>>,
}

// implement the `IndexDocument` trait for `FeatureDocument` to allow it to be indexed in Elasticsearch
impl IndexDocument for FeatureDocument {
    fn get_id(&self) -> String {
        self.feature_id.clone()
    }

    fn index_name(&self) -> String {
        "feature".to_string()
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
