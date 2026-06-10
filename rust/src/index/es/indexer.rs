//! Elasticsearch indexer module.
//!
//! Provides an Indexer struct with bulk_index<D: IndexDocument>(&mut self, docs: Vec<D>) -> Result<(), EsError> and create_indices(&self, mappings: Vec<(String, Mappings)>).
//! Integrate with client.rs for low-level HTTP calls; add batching and error handling.
//!
//! The Indexer struct is responsible for managing the indexing process, including batching documents for efficient indexing and handling any errors that may occur during the indexing process. The bulk_index method takes a vector of documents that implement the IndexDocument trait and indexes them into Elasticsearch in batches, while the create_indices method allows for the creation of indices with specified mappings. The Indexer will utilize the ElasticsearchClient from client.rs to perform the necessary HTTP calls to interact with the Elasticsearch cluster.

use crate::index::es::client::{Document, ElasticsearchClient};
use crate::index::es::config::IndexConfig;
use crate::index::es::manager::generate_index_name;
use crate::index::es::mappings::common::Mappings;
use crate::index::es::models::{EsError, IndexDocument};

// The `Indexer` struct is responsible for managing the indexing process, including batching documents for efficient indexing and handling any errors that may occur during the indexing process. It provides methods for bulk indexing documents into Elasticsearch and creating indices with specified mappings. The `Indexer` utilizes the `ElasticsearchClient` from `client.rs` to perform the necessary HTTP calls to interact with the Elasticsearch cluster.
pub struct Indexer {
    pub client: ElasticsearchClient,
    pub taxonomy: String,
    pub hub_name: String,
    pub release: String,
    pub batch_size: usize,
}

impl Indexer {
    pub fn new(
        client: ElasticsearchClient,
        taxonomy: String,
        hub_name: String,
        release: String,
    ) -> Self {
        Indexer {
            client,
            taxonomy,
            hub_name,
            release,
            batch_size: 1000, // default batch size for bulk indexing
        }
    }

    pub fn bulk_index<D: IndexDocument>(&mut self, docs: Vec<D>) -> Result<(), EsError> {
        // Implement batching logic to index documents in batches of self.batch_size
        // Use the ElasticsearchClient to perform bulk indexing operations
        // Handle any errors that may occur during the indexing process and return appropriate EsError variants
        let target_index = docs
            .first()
            .ok_or_else(|| EsError::ValidationError("No documents to index".to_string()))?
            .index_group()
            .to_string();
        let index_name =
            generate_index_name(&target_index, &self.taxonomy, &self.hub_name, &self.release);
        // if self.client.get_index_info(&index_name).is_err() {
        //     let cfg = IndexConfig {
        //         mappings: None,
        //         ..Default::default()
        //     }; // or use mapping for this doc type
        //     self.client.create_index(&index_name, cfg)?;
        // }
        for chunk in docs.chunks(self.batch_size) {
            let documents: Vec<Document> = chunk
                .iter()
                .map(|doc| {
                    doc.validate()?;
                    Ok(Document {
                        id: doc.get_id(),
                        content: serde_json::to_value(doc).map_err(|e| {
                            EsError::SerializationError(format!(
                                "Failed to serialize document: {}",
                                e.to_string()
                            ))
                        })?,
                    })
                })
                .collect::<Result<Vec<Document>, EsError>>()?;
            self.client.index_documents(&index_name, documents)?;
        }
        Ok(())
    }

    pub fn create_indices(&self, mappings: Vec<(String, Mappings)>) -> Result<(), EsError> {
        // Use the ElasticsearchClient to create indices with the specified mappings
        // Handle any errors that may occur during index creation and return appropriate EsError variants
        for (index_prefix, index_mappings) in mappings {
            let index_name =
                generate_index_name(&index_prefix, &self.taxonomy, &self.hub_name, &self.release);
            let index_config = IndexConfig {
                ..Default::default()
            };
            self.client.create_index(&index_name, index_config)?;
            if !index_mappings.properties.is_empty() {
                self.client.add_mapping(&index_name, index_mappings)?;
            }
        }
        Ok(())
    }
}

// tests for the Indexer struct and its methods
#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_bulk_index() {
        // test the bulk_index method of the Indexer struct
        setup("test_indexer_bulk_index");
        let mut indexer = Indexer::new(
            ElasticsearchClient::new("http://localhost:9200", None, None),
            "test_taxonomy".to_string(),
            "test_hub".to_string(),
            "test_release".to_string(),
        );
        let documents = vec![
            Document {
                id: "1".to_string(),
                content: serde_json::to_value("Document 1").unwrap(),
            },
            Document {
                id: "2".to_string(),
                content: serde_json::to_value("Document 2").unwrap(),
            },
        ];
        let target_prefix = documents.first().unwrap().index_group().to_string();
        let result = indexer.bulk_index(documents);
        assert!(
            result.is_ok(),
            "Failed to bulk index documents: {:?}",
            result.err()
        );
        let index_name = generate_index_name(
            &target_prefix,
            &indexer.taxonomy,
            &indexer.hub_name,
            &indexer.release,
        );
        indexer.client.refresh(&index_name).unwrap(); // refresh index to make documents searchable
        let indexed_docs = indexer
            .client
            .search(&index_name, serde_json::json!({"query": {"match_all": {}}}));
        assert!(
            indexed_docs.is_ok(),
            "Failed to search indexed documents: {:?}",
            indexed_docs.err()
        );
        let indexed_docs = indexed_docs.unwrap();
        assert_eq!(
            indexed_docs["hits"]["total"]["value"].as_i64().unwrap(),
            2,
            "Expected 2 indexed documents"
        );
    }

    #[test]
    fn test_create_indices() {
        // test the create_indices method of the Indexer struct
        setup("test_indexer_create_indices");
        let indexer = Indexer::new(
            ElasticsearchClient::new("http://localhost:9200", None, None),
            "test_taxonomy".to_string(),
            "test_hub".to_string(),
            "test_release".to_string(),
        );
        let mappings = Mappings::default();
        let result =
            indexer.create_indices(vec![("test_indexer_create_indices".to_string(), mappings)]);
        assert!(result.is_ok(), "Failed to create index: {:?}", result.err());
    }

    #[test]
    fn test_error_handling() {
        // test error handling in the Indexer methods, such as validation errors and indexing errors
    }

    #[test]
    fn test_batching() {
        // test that documents are indexed in batches according to the specified batch size
    }

    #[test]
    fn test_integration_with_client() {
        // test that the Indexer correctly interacts with the ElasticsearchClient for indexing and index creation operations
    }
}
