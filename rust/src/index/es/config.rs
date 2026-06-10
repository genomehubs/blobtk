//! Elasticsearch index config module
//! This module defines the configuration for Elasticsearch indices, including settings and mappings.
//! It provides a struct `IndexConfig` that represents the configuration for an Elasticsearch index, including settings and mappings. The `IndexConfig` struct includes fields for index settings such as number of shards and replicas, as well as a field for index mappings that defines the structure of the documents to be indexed. The module also includes functionality for validating the index configuration and ensuring that it meets the requirements for creating an Elasticsearch index.

use crate::index::es::mappings::common::Mappings;
use serde::{Deserialize, Serialize};

// The IndexSettings struct represents the settings for an Elasticsearch index, including the number of shards and replicas
// It includes analysis settings such as analyzers, tokenizers, and filters that can be used to customize the indexing and search behavior of the index

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalysisSettings {
    pub analyzer: Option<serde_json::Value>,
    pub filter: Option<serde_json::Value>,
    pub tokenizer: Option<serde_json::Value>,
}

// set default analysis settings for the index
impl Default for AnalysisSettings {
    fn default() -> Self {
        AnalysisSettings {
            analyzer: Some(serde_json::json!({
                "trigram": {
                    "type": "custom",
                    "tokenizer": "standard",
                    "filter": ["lowercase", "shingle"]
                },
                "reverse": {
                    "type": "custom",
                    "tokenizer": "standard",
                    "filter": ["lowercase", "reverse"]
                }
            })),
            filter: Some(serde_json::json!({
                "shingle": {
                    "type": "shingle",
                    "min_shingle_size": 2,
                    "max_shingle_size": 3
                }
            })),
            tokenizer: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexSettings {
    pub number_of_shards: u32,
    pub number_of_replicas: u32,
    pub analysis: AnalysisSettings,
}

// use default index settings if not provided
impl Default for IndexSettings {
    fn default() -> Self {
        IndexSettings {
            number_of_shards: 1,
            number_of_replicas: 0,
            analysis: AnalysisSettings::default(),
        }
    }
}

// the IndexConfig struct represents the configuration for an Elasticsearch index, including settings and mappings
// it includes fields for index settings such as number of shards and replicas, as well as a field for index mappings that defines the structure of the documents to be indexed
// this struct will include methods for validating the index configuration and ensuring that it meets the requirements for creating an Elasticsearch index

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IndexConfig {
    pub settings: IndexSettings,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mappings: Option<Mappings>,
}
