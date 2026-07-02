//!  typed document interface used by indexer and builders
//!
//! The IndexDocument trait defines the structure of a document that can be indexed in Elasticsearch. It includes fields for the document ID, content, and any additional metadata that may be relevant for indexing and searching. The trait also includes methods for serializing and deserializing the document to and from JSON format, as well as any necessary validation or transformation logic to ensure that the document is properly formatted for indexing in Elasticsearch.

use serde::{Deserialize, Serialize};

// The `models` module defines the data structures and types used in the Elasticsearch indexing process. It includes definitions for the index configuration, index mappings, and document structure that will be indexed into Elasticsearch. The module also includes any necessary helper functions or traits for working with these data structures, such as serialization and deserialization logic for converting between Rust structs and JSON format for indexing into Elasticsearch.
pub mod documents;
pub mod nested_documents;

pub trait IndexDocument: Serialize {
    fn get_id(&self) -> String;
    fn index_group(&self) -> IndexGroup;
    fn validate(&self) -> Result<(), EsError>;
}

pub trait BuildDocument: Serialize {
    fn add_attribute(
        &mut self,
        attribute: nested_documents::NestedAttribute,
    ) -> Result<(), EsError>;
}

#[derive(Debug)]
pub enum EsError {
    ValidationError(String),
    IndexingError(String),
    ConnectionError(String),
    ApiError(String),
    SerializationError(String),
    Other(String),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndexGroup {
    Feature,
    #[default]
    Taxon,
    Assembly,
    Sample,
    Attribute,
    None,
}

impl std::fmt::Display for IndexGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            IndexGroup::Feature => "feature",
            IndexGroup::Taxon => "taxon",
            IndexGroup::Assembly => "assembly",
            IndexGroup::Sample => "sample",
            IndexGroup::Attribute => "attribute",
            IndexGroup::None => "none",
        };
        write!(f, "{}", s)
    }
}
