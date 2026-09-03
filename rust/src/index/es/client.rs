//! Elasticsearch client module
//! This module defines the Elasticsearch client that will be used to interact with the Elasticsearch cluster. It provides functionality for connecting to the cluster, performing API calls, and handling responses.
//! The `ElasticsearchClient` struct represents the client for interacting with the Elasticsearch cluster. It includes fields for the cluster URL, authentication credentials, and any necessary configuration options. The module also includes methods for performing API calls to the Elasticsearch cluster, such as creating indices, indexing documents, and performing search operations. Additionally, it includes error handling to manage any issues that may arise during interactions with the Elasticsearch cluster.
//! Example usage:
//! ```rust//! use crate::index::es::client::ElasticsearchClient;
//! let client = ElasticsearchClient::new("http://localhost:9200", Some("username".to_string()), Some("password".to_string()));
//! ```
//! This module serves as the foundation for all interactions with the Elasticsearch cluster, providing a robust and flexible interface for managing indices, indexing documents, and performing search operations. It is designed to be easily extensible, allowing for additional functionality to be added as needed while maintaining a clear and consistent interface for users of the module.

use serde::{Deserialize, Serialize};

use crate::import::EsConfig;
use crate::index::es::config::IndexConfig;
use crate::index::es::mappings::common::Mappings;
use crate::index::es::models::{EsError, IndexDocument, IndexGroup};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexInfo {
    pub name: String,
    pub aliases: serde_json::Value,
    pub mappings: serde_json::Value,
    pub settings: serde_json::Value,
}

impl IndexInfo {
    pub fn from_json(json: &serde_json::Value) -> Option<Self> {
        let name = json.as_object()?.keys().next()?.to_string();
        let index_info = json.get(&name)?;
        Some(IndexInfo {
            name,
            aliases: index_info.get("aliases")?.clone(),
            mappings: index_info.get("mappings")?.clone(),
            settings: index_info.get("settings")?.clone(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub content: serde_json::Value,
}

impl IndexDocument for Document {
    fn get_id(&self) -> String {
        self.id.clone()
    }

    fn index_group(&self) -> IndexGroup {
        // This method should return the name of the index that the document belongs to.
        // the index name is the first part of the document ID, - separator.
        // For example, if the document ID is "feature-12345", the index name would be "feature".
        match self.id.split('-').next().unwrap_or("default_index") {
            "feature" => IndexGroup::Feature,
            "taxon" => IndexGroup::Taxon,
            "assembly" => IndexGroup::Assembly,
            "sample" => IndexGroup::Sample,
            "attribute" => IndexGroup::Attribute,
            _ => IndexGroup::None, // default to None if unknown
        }
    }

    fn validate(&self) -> Result<(), EsError> {
        // Implement validation logic for the document content here.
        // This could include checking for required fields, ensuring that the content is in the correct format, etc.
        // If the document is valid, return Ok(()). If there are validation errors, return an appropriate EsError variant with details about the validation failure.
        Ok(())
    }
}

pub struct ElasticsearchClient {
    pub cluster_url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub index_suffix: Option<String>,
}

impl ElasticsearchClient {
    pub fn new(cluster_url: &str, username: Option<String>, password: Option<String>) -> Self {
        ElasticsearchClient {
            cluster_url: cluster_url.to_string(),
            username,
            password,
            index_suffix: None,
        }
    }

    pub fn set_index_suffix(&mut self, suffix: &str) {
        self.index_suffix = Some(suffix.to_string());
    }

    pub fn resolve_index_name(&self, index_prefix: &str) -> Result<String, EsError> {
        if let Some(suffix) = &self.index_suffix {
            Ok(format!("{}{}", index_prefix, suffix))
        } else {
            Err(EsError::ApiError(
                "Index suffix is not set. Cannot determine full index name.".to_string(),
            ))
        }
    }

    // check if the Elasticsearch cluster is reachable and the credentials are valid
    pub fn check_connection(&self) -> Result<(), String> {
        let client = reqwest::blocking::Client::new();
        let response = client
            .get(&self.cluster_url)
            .send()
            .map_err(|e| e.to_string())?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!(
                "Failed to connect to Elasticsearch cluster: {}",
                response.text().unwrap_or_default()
            ))
        }
    }

    // get cluster health status
    pub fn get_cluster_health(&self) -> Result<String, String> {
        let client = reqwest::blocking::Client::new();
        let response = client
            .get(&format!("{}/_cluster/health", self.cluster_url))
            .send()
            .map_err(|e| e.to_string())?;
        if response.status().is_success() {
            let health_status: serde_json::Value = response.json().map_err(|e| e.to_string())?;
            Ok(health_status["status"]
                .as_str()
                .unwrap_or_default()
                .to_string())
        } else {
            Err(format!(
                "Failed to get cluster health: {}",
                response.text().unwrap_or_default()
            ))
        }
    }

    // get all indices in the cluster
    pub fn get_all_indices(&self) -> Result<Vec<String>, String> {
        let client = reqwest::blocking::Client::new();
        let response = client
            .get(&format!("{}/_cat/indices?h=index", self.cluster_url))
            .send()
            .map_err(|e| e.to_string())?;
        if response.status().is_success() {
            let indices: Vec<String> = response
                .text()
                .map_err(|e| e.to_string())?
                .lines()
                .map(|line| line.trim().to_string())
                .collect();
            Ok(indices)
        } else {
            Err(format!(
                "Failed to get indices: {}",
                response.text().unwrap_or_default()
            ))
        }
    }

    // create an index using the elasticsearch API
    pub fn create_index(&self, index_name: &str, index_config: IndexConfig) -> Result<(), EsError> {
        // generate the API request to create the index with the specified name and configuration
        let request_url = format!("{}/{}", self.cluster_url, index_name);
        let request_body = serde_json::to_string(&index_config)
            .map_err(|e| EsError::SerializationError(e.to_string()))?;
        // send the request using the reqwest crate and handle the response
        let client = reqwest::blocking::Client::new();
        let response = client
            .put(&request_url)
            .header("Content-Type", "application/json")
            .body(request_body)
            .send()
            .map_err(|e| EsError::ApiError(e.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(EsError::ApiError(format!(
                "Failed to create index: {}",
                response.text().unwrap_or_default()
            )))
        }
    }

    pub fn wait_for_index_ready(
        &self,
        index_name: &str,
        minimum_status: &str,
    ) -> Result<(), EsError> {
        let minimum = minimum_status.to_ascii_lowercase();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let client = reqwest::blocking::Client::new();

        loop {
            let response = client
                .get(&format!(
                    "{}/_cluster/health/{}",
                    self.cluster_url, index_name
                ))
                .send()
                .map_err(|e| EsError::ApiError(e.to_string()))?;

            if response.status().is_success() {
                let json: serde_json::Value = response
                    .json()
                    .map_err(|e| EsError::SerializationError(e.to_string()))?;
                let status = json
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                eprintln!("  ES status for {}: {}", index_name, status);

                if matches!(status.to_ascii_lowercase().as_str(), "green" | "yellow")
                    && matches!(minimum.as_str(), "green" | "yellow")
                {
                    let rank = match status.to_ascii_lowercase().as_str() {
                        "green" => 2,
                        "yellow" => 1,
                        _ => 0,
                    };
                    let min_rank = match minimum.as_str() {
                        "green" => 2,
                        "yellow" => 1,
                        _ => 0,
                    };
                    if rank >= min_rank {
                        eprintln!("  Index {} ready with status {}", index_name, status);
                        return Ok(());
                    }
                }
            } else {
                eprintln!(
                    "  Index {} not ready yet (health request failed): {}",
                    index_name,
                    response
                        .text()
                        .unwrap_or_else(|_| "unknown error".to_string())
                );
            }

            if std::time::Instant::now() >= deadline {
                return Err(EsError::ApiError(format!(
                    "Timed out waiting for index {} to reach status {}",
                    index_name, minimum_status
                )));
            }

            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    pub fn add_mapping(&self, index_name: &str, mapping: Mappings) -> Result<(), EsError> {
        // generate the API request to add the mapping to the specified index
        let request_url = format!("{}/{}/_mapping", self.cluster_url, index_name);
        let request_body = serde_json::to_string(&mapping)
            .map_err(|e| EsError::SerializationError(e.to_string()))?;
        // send the request using the reqwest crate and handle the response
        let client = reqwest::blocking::Client::new();
        let response = client
            .put(&request_url)
            .header("Content-Type", "application/json")
            .body(request_body)
            .send()
            .map_err(|e| EsError::ApiError(e.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(EsError::ApiError(format!(
                "Failed to add mapping: {}",
                response.text().unwrap_or_default()
            )))
        }
    }

    pub fn delete_index(&self, index_name: &str) -> Result<(), EsError> {
        // generate the API request to delete the index with the specified name
        let request_url = format!("{}/{}", self.cluster_url, index_name);
        // send the request using the reqwest crate and handle the response
        let client = reqwest::blocking::Client::new();
        let response = client
            .delete(&request_url)
            .send()
            .map_err(|e| EsError::ApiError(e.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(EsError::ApiError(format!(
                "Failed to delete index: {}",
                response.text().unwrap_or_default()
            )))
        }
    }

    pub fn get_index_info(&self, index_name: &str) -> Result<IndexInfo, EsError> {
        // generate the API request to retrieve information about the index with the specified name
        let request_url = format!("{}/{}", self.cluster_url, index_name);
        // send the request using the reqwest crate and handle the response
        let client = reqwest::blocking::Client::new();
        let response = client
            .get(&request_url)
            .send()
            .map_err(|e| EsError::ApiError(e.to_string()))?;
        if response.status().is_success() {
            let json: serde_json::Value = response
                .json()
                .map_err(|e| EsError::SerializationError(e.to_string()))?;
            let index_info = IndexInfo::from_json(&json)
                .ok_or(EsError::ApiError("Failed to parse index info".to_string()))?;
            Ok(index_info)
        } else {
            Err(EsError::ApiError(format!(
                "Failed to get index info: {}",
                response.text().unwrap_or_default()
            )))
        }
    }

    pub fn index_document(&self, index_name: &str, document: Document) -> Result<(), EsError> {
        // generate the API request to index the document into the specified index
        let request_url = format!("{}/{}/_doc", self.cluster_url, index_name);
        // serialize the document content to JSON format for indexing
        let request_body = serde_json::to_string(&document)
            .map_err(|e| EsError::SerializationError(e.to_string()))?;
        // send the request using the reqwest crate and handle the response
        let client = reqwest::blocking::Client::new();
        let response = client
            .post(&request_url)
            .header("Content-Type", "application/json")
            .body(request_body)
            .send()
            .map_err(|e| EsError::ApiError(e.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(EsError::ApiError(format!(
                "Failed to index document: {}",
                response.text().unwrap_or_default()
            )))
        }
    }

    /// Convert any IndexDocument to the wrapper Document type
    pub fn wrap_for_bulk_index<T: IndexDocument>(
        &self,
        docs: Vec<T>,
    ) -> Result<Vec<Document>, EsError> {
        docs.into_iter()
            .map(|doc| {
                Ok(Document {
                    id: doc.get_id(),
                    content: serde_json::to_value(&doc)
                        .map_err(|e| EsError::SerializationError(e.to_string()))?,
                })
            })
            .collect()
    }

    pub fn ensure_index_exists_for_prefix(&self, index_prefix: &str) -> Result<(), EsError> {
        let index_name = self.resolve_index_name(index_prefix)?;

        if self.get_index_info(&index_name).is_ok() {
            return Ok(());
        }

        let mappings = match index_prefix {
            "feature" => crate::index::es::mappings::feature_index_mappings(),
            "attributes" => crate::index::es::mappings::attribute_index_mappings(),
            _ => Mappings::default(),
        };

        let cfg = IndexConfig {
            settings: Default::default(),
            mappings: Some(mappings),
        };

        match self.create_index(&index_name, cfg) {
            Ok(()) => {
                eprintln!(
                    "  Created index {} and waiting for it to become ready",
                    index_name
                );
                self.wait_for_index_ready(&index_name, "yellow")?;
                Ok(())
            }
            Err(err)
                if err.to_string().contains("already exists")
                    || err
                        .to_string()
                        .contains("resource_already_exists_exception") =>
            {
                eprintln!("  Index {} already exists; checking readiness", index_name);
                self.wait_for_index_ready(&index_name, "yellow")?;
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    pub fn index_documents(
        &self,
        index_prefix: &str,
        documents: Vec<Document>,
    ) -> Result<(), EsError> {
        if documents.is_empty() {
            return Ok(());
        }

        self.ensure_index_exists_for_prefix(index_prefix)?;

        // generate the API request to perform bulk indexing of the documents into the specified index
        // keep batches modest to avoid request timeouts on busy clusters
        let batch_size = 1000;
        for chunk in documents.chunks(batch_size) {
            self.index_documents_chunk(index_prefix, chunk.to_vec())?;
        }
        Ok(())
    }

    fn index_documents_chunk(
        &self,
        index_prefix: &str,
        documents: Vec<Document>,
    ) -> Result<(), EsError> {
        let index_name = self.resolve_index_name(index_prefix)?;
        let request_url = format!("{}/{}/_bulk", self.cluster_url, index_name);
        let mut bulk_request_body = String::new();

        for document in documents {
            let doc_id = if document.id.starts_with(index_prefix) {
                document.id.clone()
            } else {
                format!("{}-{}", index_prefix, document.id)
            };
            let action = serde_json::json!({
                "index": {
                    "_index": index_name,
                    "_id": doc_id,
                }
            });
            let action_str = serde_json::to_string(&action)
                .map_err(|e| EsError::SerializationError(e.to_string()))?;
            let document_str = serde_json::to_string(&document.content)
                .map_err(|e| EsError::SerializationError(e.to_string()))?;
            bulk_request_body.push_str(&format!("{}\n{}\n", action_str, document_str));
        }
        // send the request using the reqwest crate and handle the response
        let client = reqwest::blocking::Client::new();
        let response = client
            .post(&request_url)
            .header("Content-Type", "application/json")
            .body(bulk_request_body)
            .send()
            .map_err(|e| EsError::ApiError(e.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(EsError::ApiError(format!(
                "Failed to perform bulk indexing: {}",
                response.text().unwrap_or_default()
            )))
        }
    }

    pub fn refresh(&self, index_prefix: &str) -> Result<(), EsError> {
        let index_name = self.resolve_index_name(index_prefix)?;
        let url = format!("{}/{}/_refresh", self.cluster_url, index_name);
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(&url)
            .send()
            .map_err(|e| EsError::ApiError(e.to_string()))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(EsError::ApiError(resp.text().unwrap_or_default()))
        }
    }

    pub fn search(
        &self,
        index_name: &str,
        query: serde_json::Value,
    ) -> Result<serde_json::Value, EsError> {
        // generate the API request to perform a search operation on the specified index with the given query
        let request_url = format!("{}/{}/_search", self.cluster_url, index_name);
        let request_body = serde_json::to_string(&query)
            .map_err(|e| EsError::SerializationError(e.to_string()))?;
        // send the request using the reqwest crate and handle the response
        let client = reqwest::blocking::Client::new();
        let response = client
            .post(&request_url)
            .header("Content-Type", "application/json")
            .body(request_body)
            .send()
            .map_err(|e| EsError::ApiError(e.to_string()))?;
        if response.status().is_success() {
            let search_results: serde_json::Value = response
                .json()
                .map_err(|e| EsError::SerializationError(e.to_string()))?;
            Ok(search_results)
        } else {
            Err(EsError::ApiError(format!(
                "Failed to perform search: {}",
                response.text().unwrap_or_default()
            )))
        }
    }
}

impl TryFrom<&EsConfig> for ElasticsearchClient {
    type Error = EsError;

    fn try_from(value: &EsConfig) -> Result<Self, Self::Error> {
        let index_suffix = format!(
            "--{}--{}--{}",
            value.hub.taxonomy, value.hub.name, value.hub.release
        );
        let mut client = ElasticsearchClient::new(
            &format!("{}:{}", value.host, value.port),
            value.username.clone(),
            value.password.clone(),
        );
        client.set_index_suffix(&index_suffix);
        Ok(client)
    }
}
