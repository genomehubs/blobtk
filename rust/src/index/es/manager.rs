//! Elasticsearch index manager.
//! This module provides functionality for managing Elasticsearch indices, including creating, updating, and deleting indices,
//! as well as handling index mappings and settings.

use crate::index::es::client::{Document, ElasticsearchClient, IndexInfo};
use crate::index::es::config::IndexConfig;
use crate::index::es::models::EsError;

// The `IndexManager` struct is responsible for managing Elasticsearch indices, including creating, updating, and deleting indices,
// as well as handling index mappings and settings. It provides a high-level interface for interacting with Elasticsearch indices,
// allowing users to easily manage their indices and perform operations such as indexing documents and performing search operations.

pub struct IndexManager {
    // Fields for managing Elasticsearch indices, such as client configuration, index settings, and mappings.
    // This struct will include methods for creating, updating, and deleting indices, as well as handling index mappings and settings.
    pub client: ElasticsearchClient,
    pub index_name: String,
    pub index_config: IndexConfig,
}

impl IndexManager {
    pub fn new(client: ElasticsearchClient, index_name: String, index_config: IndexConfig) -> Self {
        IndexManager {
            client,
            index_name,
            index_config,
        }
    }

    pub fn create_index(&self) -> Result<(), EsError> {
        // Use the ElasticsearchClient to create an index with the specified name and configuration.
        let result = self
            .client
            .create_index(&self.index_name, self.index_config.clone());
        result
    }

    pub fn delete_index(&self) -> Result<(), EsError> {
        // Use the ElasticsearchClient to delete the index with the specified name.
        let result = self.client.delete_index(&self.index_name);
        result
    }

    pub fn get_index_info(&self) -> Result<IndexInfo, EsError> {
        // Use the ElasticsearchClient to retrieve information about the index with the specified name.
        let result = self.client.get_index_info(&self.index_name);
        result
    }

    pub fn index_document(&self, document: Document) -> Result<(), EsError> {
        // Use the ElasticsearchClient to index the document into the specified index.
        let result = self.client.index_document(&self.index_name, document);
        result
    }
}

// helper function to generate an index name based on a given prefix, taxonomy, hub name and release
pub fn generate_index_name(prefix: &str, taxonomy: &str, hub_name: &str, release: &str) -> String {
    format!("{}--{}--{}--{}", prefix, taxonomy, hub_name, release)
}

// tests for the IndexManager struct and its methods
#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::es::config::{IndexConfig, IndexSettings};

    fn setup(index_name: &str) {
        // set up the test environment
        // check elasticsearch instance is running and accessible
        let client = ElasticsearchClient::new("http://localhost:9200", None, None);
        let response = client.get_cluster_health();
        assert!(
            response.is_ok(),
            "Elasticsearch instance is not running or accessible"
        );

        // clean up any existing test indices
        let existing_indices = client.get_all_indices();
        if let Ok(indices) = existing_indices {
            for index in indices {
                if index.starts_with(index_name) {
                    client.delete_index(&index).unwrap();
                }
            }
        }
    }

    #[test]
    fn test_generate_index_name() {
        let prefix = "my_index";
        let taxonomy = "my_taxonomy";
        let hub_name = "my_hub";
        let release = "2026.06.02";
        let expected_index_name = "my_index--my_taxonomy--my_hub--2026.06.02";
        let generated_index_name = generate_index_name(prefix, taxonomy, hub_name, release);
        assert_eq!(generated_index_name, expected_index_name);
    }

    #[test]
    fn test_create_index() {
        let index_name = "test_index_create".to_string();
        setup(&index_name);
        let client = ElasticsearchClient::new("http://localhost:9200", None, None);
        let index_config = IndexConfig {
            settings: IndexSettings {
                number_of_shards: 1,
                number_of_replicas: 0,
                ..Default::default()
            },
            mappings: None,
        };
        let index_manager = IndexManager::new(client, index_name, index_config);
        let result = index_manager.create_index();
        assert!(result.is_ok());
    }

    #[test]
    fn test_delete_index() {
        let index_name = "test_index_delete".to_string();
        setup(&index_name);
        let client = ElasticsearchClient::new("http://localhost:9200", None, None);
        let index_config = IndexConfig {
            settings: IndexSettings {
                number_of_shards: 1,
                number_of_replicas: 0,
                ..Default::default()
            },
            mappings: None,
        };
        let index_manager = IndexManager::new(client, index_name, index_config);
        let create_result = index_manager.create_index();
        assert!(create_result.is_ok());
        let delete_result = index_manager.delete_index();
        assert!(delete_result.is_ok());
    }

    #[test]
    fn test_get_index_info() {
        let index_name = "test_index_info".to_string();
        setup(&index_name);
        let client = ElasticsearchClient::new("http://localhost:9200", None, None);
        let index_config = IndexConfig {
            settings: IndexSettings {
                number_of_shards: 1,
                number_of_replicas: 0,
                ..Default::default()
            },
            mappings: None,
        };
        let index_manager = IndexManager::new(client, index_name.clone(), index_config.clone());
        let create_result = index_manager.create_index();
        assert!(create_result.is_ok());
        let index_info_result = index_manager.get_index_info();
        assert!(index_info_result.is_ok());
        let index_info = &index_info_result.unwrap();
        assert_eq!(index_info.name, index_name);
        let index_settings = index_info.settings.get("index").and_then(|v| v.as_object());
        assert!(
            index_settings.is_some(),
            "Index settings not found in index info"
        );
        let index_settings = index_settings.unwrap();
        dbg!(&index_settings);
        // Need to convert numer of shards from string to u64
        let index_number_of_shards = index_settings
            .get("number_of_shards")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap();
        let index_number_of_replicas = index_settings
            .get("number_of_replicas")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap();
        dbg!(&index_number_of_shards, &index_number_of_replicas);
        assert_eq!(
            index_number_of_shards,
            index_config.settings.number_of_shards as u64
        );
        assert_eq!(
            index_number_of_replicas,
            index_config.settings.number_of_replicas as u64
        );
    }

    #[test]
    fn test_index_document() {
        let index_name = "test_index_document".to_string();
        setup(&index_name);
        let client = ElasticsearchClient::new("http://localhost:9200", None, None);
        let index_config = IndexConfig {
            settings: IndexSettings {
                number_of_shards: 1,
                number_of_replicas: 0,
                ..Default::default()
            },
            mappings: None,
        };
        let index_manager = IndexManager::new(client, index_name, index_config);
        let create_result = index_manager.create_index();
        assert!(create_result.is_ok());
        let document = Document {
            id: "1".to_string(),
            content: serde_json::json!({"content": "Test document content"}),
        };
        let index_result = index_manager.index_document(document);
        assert!(index_result.is_ok());
    }
}
