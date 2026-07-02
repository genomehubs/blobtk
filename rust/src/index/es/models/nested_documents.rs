//! Define nesteddocuments, which are structured representations of data for indexing and searching in Elasticsearch.

use serde::{Deserialize, Serialize};

use crate::parse::genomehubs::StringOrVec;

// Defines the structure of a nested ontology entry for indexing and searching nested documents in Elasticsearch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NestedOntology {
    pub ontology_term: String,
    pub ontology_id: String,
}

// Defines the structure of nested values mirroring the shared_value_propoerties of the corresponding mappings, for use in indexing and searching nested documents in Elasticsearch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NestedAttributeValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyword_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flattened_value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo_point_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo_hex_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo_tile_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bool_value: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_value: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integer_value: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_value: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_value: Option<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub double_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub float_value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub half_float_value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_dp_value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub two_dp_value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub three_dp_value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub four_dp_value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_primary_value: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ontology_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ontology: Option<NestedOntology>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_doc_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_year: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_doi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_pubmed_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_prefix: Option<String>,
}

// Defines the structure of a nested attribute for indexing and searching nested documents in Elasticsearch.
// includes all the shared non-value properties in addition to the NestedAttributeValue keys.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NestedAttribute {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyword_value: Option<StringOrVec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flattened_value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo_point_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo_hex_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo_tile_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bool_value: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_value: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integer_value: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_value: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_value: Option<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub double_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub float_value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub half_float_value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_dp_value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub two_dp_value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub three_dp_value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub four_dp_value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_primary_value: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ontology_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ontology: Option<NestedOntology>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_doc_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_year: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_doi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_pubmed_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url_template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregation_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregation_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mean: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdev: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hexbin1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hexbin2: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hexbin3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hexbin4: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hexbin5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hexbin6: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<NestedAttributeValue>>,
}

// Defines the structure of a nested identifier for indexing and searching nested documents in Elasticsearch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NestedIdentifier {
    pub identifier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url_template: Option<String>,
}

// Defines the structure of a nested lineage for indexing and searching nested documents in Elasticsearch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NestedLineage {
    pub taxon_id: String,
    pub taxon_rank: Option<String>,
    pub scientific_name: String,
    pub node_depth: f32,
    pub support_value: Option<f32>,
}

//Defines the structure of a nested taxon name for indexing and searching nested documents in Elasticsearch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NestedTaxonName {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url_stub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url_template: Option<String>,
}
