//!  typed document interface used by indexer and builders
//!
//! The IndexDocument trait defines the structure of a document that can be indexed in Elasticsearch. It includes fields for the document ID, content, and any additional metadata that may be relevant for indexing and searching. The trait also includes methods for serializing and deserializing the document to and from JSON format, as well as any necessary validation or transformation logic to ensure that the document is properly formatted for indexing in Elasticsearch.
//! It supports (fn get_id(&self)->String, fn index_name(&self)->&'static str, fn validate(&self)->Result<(),EsError>), EntityId enum, EsError enum.

use serde::Serialize;

// The `models` module defines the data structures and types used in the Elasticsearch indexing process. It includes definitions for the index configuration, index mappings, and document structure that will be indexed into Elasticsearch. The module also includes any necessary helper functions or traits for working with these data structures, such as serialization and deserialization logic for converting between Rust structs and JSON format for indexing into Elasticsearch.
pub mod documents;

pub trait IndexDocument: Serialize {
    fn get_id(&self) -> String;
    fn index_name(&self) -> String;
    fn validate(&self) -> Result<(), EsError>;
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

#[derive(Debug)]
pub enum EntityId {
    Feature(String),
    Sequence(String),
    Assembly(String),
    Analysis(String),
    File(String),
    Taxon(String),
}
