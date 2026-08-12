//! Common mappings for Elasticsearch indexing. This module defines the common mappings for Elasticsearch indices, including the structure of the documents to be indexed and the field types.

use std::collections::HashMap;

use serde::{ser::SerializeMap, Deserialize, Serialize};

#[derive(Deserialize, Debug, Clone)]
pub struct PropertyMeta {
    pub description: String,
}

// Restrict length of property meta description to 50 chars when serializing
impl Serialize for PropertyMeta {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let truncated_description = if self.description.len() > 50 {
            let truncated = format!("{}...", &self.description[..47]);
            truncated
        } else {
            self.description.clone()
        };
        let meta = PropertyMeta {
            description: truncated_description,
        };
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("description", &meta.description)?;
        map.end()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Normalizer {
    Lowercase,
    Uppercase,
    AsciiFolding,
    Custom(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Analyzer {
    Standard,
    Simple,
    Whitespace,
    Stop,
    Keyword,
    Trigram,
    Reverse,
    Pattern(String),
    Custom(String),
}

// The `Property` struct represents a property in the mapping for an Elasticsearch index
// It includes fields for the name and field type of the property and supports top level and nested properties
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Property {
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<PropertyMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, Property>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalizer: Option<Normalizer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_above: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<std::collections::HashMap<String, Property>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scaling_factor: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eager_global_ordinals: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub null_value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyzer: Option<Analyzer>,
}

// The `Mappings` struct represents the mapping for an Elasticsearch index
// It includes a field for the properties of the mapping that defines the structure of the documents to be indexed
// This struct will include methods for validating the index mapping and ensuring that it meets the requirements for
// creating an Elasticsearch index
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Mappings {
    pub dynamic: Option<bool>,
    pub properties: HashMap<String, Property>,
}

impl Mappings {
    pub fn new(properties: HashMap<String, Property>, dynamic: Option<bool>) -> Self {
        Mappings {
            dynamic,
            properties,
        }
    }

    pub fn add_property(&mut self, name: String, property: Property) {
        self.properties.insert(name, property);
    }

    pub fn get_property(&self, name: &str) -> Option<&Property> {
        self.properties.get(name)
    }

    pub fn remove_property(&mut self, name: &str) {
        self.properties.remove(name);
    }

    pub fn validate(&self) -> Result<(), String> {
        // Implement validation logic for the mappings, such as checking for required fields, ensuring that field types are valid, and verifying that nested properties are properly defined
        // Return Ok(()) if the mappings are valid, or Err(String) with an error message if the mappings are invalid
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

// reusable property helpers to simplify construction of mappings

pub fn keyword_property(
    description: &str,
    ignore_above: Option<u32>,
    normalizer: Option<Normalizer>,
) -> Property {
    Property {
        field_type: "keyword".to_string(),
        ignore_above,
        normalizer,
        meta: Some(PropertyMeta {
            description: description.to_string(),
        }),
        ..Default::default()
    }
}

pub fn keyword_with_null_value_property(
    description: &str,
    ignore_above: Option<u32>,
    normalizer: Option<Normalizer>,
    null_value: Option<String>,
) -> Property {
    Property {
        field_type: "keyword".to_string(),
        ignore_above,
        normalizer,
        null_value: null_value.map(serde_json::Value::String),
        meta: Some(PropertyMeta {
            description: description.to_string(),
        }),
        ..Default::default()
    }
}

pub fn search_as_you_type_property(description: &str) -> Property {
    Property {
        field_type: "search_as_you_type".to_string(),
        meta: Some(PropertyMeta {
            description: description.to_string(),
        }),
        ..Default::default()
    }
}

pub fn keyword_with_fields_property(
    description: &str,
    ignore_above: Option<u32>,
    normalizer: Option<Normalizer>,
    fields: HashMap<String, Property>,
) -> Property {
    Property {
        field_type: "keyword".to_string(),
        ignore_above,
        normalizer,
        fields: Some(fields),
        meta: Some(PropertyMeta {
            description: description.to_string(),
        }),
        ..Default::default()
    }
}

pub fn keyword_with_lookup_property(
    description: &str,
    ignore_above: Option<u32>,
    normalizer: Option<Normalizer>,
) -> Property {
    keyword_with_fields_property(
        description,
        ignore_above,
        normalizer,
        HashMap::from([
            ("text".to_string(), text_property("text", None, None)),
            ("raw".to_string(), keyword_property("keyword", None, None)),
            ("live".to_string(), search_as_you_type_property("live")),
            (
                "trigram".to_string(),
                text_property("trigram", None, Some(Analyzer::Trigram)),
            ),
            (
                "reverse".to_string(),
                text_property("reverse", None, Some(Analyzer::Reverse)),
            ),
        ]),
    )
}

pub fn flattened_property(description: &str, eager_global_ordinals: Option<bool>) -> Property {
    Property {
        field_type: "flattened".to_string(),
        eager_global_ordinals,
        meta: Some(PropertyMeta {
            description: description.to_string(),
        }),
        ..Default::default()
    }
}

pub fn text_property(
    description: &str,
    index: Option<bool>,
    analyzer: Option<Analyzer>,
) -> Property {
    Property {
        field_type: "text".to_string(),
        index,
        analyzer,
        meta: Some(PropertyMeta {
            description: description.to_string(),
        }),
        ..Default::default()
    }
}

pub fn numeric_property(
    description: &str,
    field_type: &str,
    scaling_factor: Option<u32>,
) -> Property {
    Property {
        field_type: field_type.to_string(),
        meta: Some(PropertyMeta {
            description: description.to_string(),
        }),
        scaling_factor,
        ..Default::default()
    }
}

pub fn boolean_property(description: &str) -> Property {
    Property {
        field_type: "boolean".to_string(),
        meta: Some(PropertyMeta {
            description: description.to_string(),
        }),
        ..Default::default()
    }
}

pub fn date_property(description: &str) -> Property {
    Property {
        field_type: "date".to_string(),
        meta: Some(PropertyMeta {
            description: description.to_string(),
        }),
        ..Default::default()
    }
}

pub fn geo_point_property(description: &str) -> Property {
    Property {
        field_type: "geo_point".to_string(),
        meta: Some(PropertyMeta {
            description: description.to_string(),
        }),
        ..Default::default()
    }
}

pub fn object_property(description: &str) -> Property {
    Property {
        field_type: "object".to_string(),
        index: Some(false),
        meta: Some(PropertyMeta {
            description: description.to_string(),
        }),
        ..Default::default()
    }
}

pub fn nested_property(properties: HashMap<String, Property>) -> Property {
    Property {
        field_type: "nested".to_string(),
        properties: Some(properties),
        ..Default::default()
    }
}

// Set of shared properties for taxon, assembly, sample and feature indices
// present in both top level and nested "values" mapping in the taxon index
pub fn shared_value_properties() -> HashMap<String, Property> {
    HashMap::from([
        (
            "keyword_value".to_string(),
            keyword_with_fields_property(
                "Value of type keyword (including ontology terms)",
                Some(64),
                Some(Normalizer::Lowercase),
                HashMap::from([("raw".to_string(), keyword_property("raw", None, None))]),
            ),
        ),
        (
            "flattened_value".to_string(),
            flattened_property("Value of type flattened", Some(true)),
        ),
        (
            "text_value".to_string(),
            text_property("Value of type text", None, None),
        ),
        (
            "geo_point_value".to_string(),
            geo_point_property("Value of type geo_point"),
        ),
        (
            "geo_hex_value".to_string(),
            keyword_property(
                "Value of type geo_hex, stored as a keyword",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "geo_tile_value".to_string(),
            keyword_property(
                "Value of type geo_tile, stored as a keyword",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "date_value".to_string(),
            date_property("Value of type date"),
        ),
        (
            "bool_value".to_string(),
            boolean_property("Value can be true or false"),
        ),
        (
            "long_value".to_string(),
            numeric_property("Value of type long", "long", None),
        ),
        (
            "integer_value".to_string(),
            numeric_property("Value of type integer", "integer", None),
        ),
        (
            "short_value".to_string(),
            numeric_property("Value of type short", "short", None),
        ),
        (
            "byte_value".to_string(),
            numeric_property("Value of type byte", "byte", None),
        ),
        (
            "double_value".to_string(),
            numeric_property("Value of type double", "double", None),
        ),
        (
            "float_value".to_string(),
            numeric_property("Value of type float", "float", None),
        ),
        (
            "half_float_value".to_string(),
            numeric_property("Value of type half_float", "half_float", None),
        ),
        (
            "1dp_value".to_string(),
            numeric_property("Value to 1 dp", "scaled_float", Some(10)),
        ),
        (
            "2dp_value".to_string(),
            numeric_property("Value to 2 dp", "scaled_float", Some(100)),
        ),
        (
            "3dp_value".to_string(),
            numeric_property("Value to 3 dp", "scaled_float", Some(1000)),
        ),
        (
            "4dp_value".to_string(),
            numeric_property("Value to 4 dp", "scaled_float", Some(10000)),
        ),
        (
            "is_primary_value".to_string(),
            boolean_property("Indicates if the value is primary"),
        ),
        (
            "ontology_id".to_string(),
            keyword_property(
                "Ontology ID (with matching term in keyword_value)",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "ontology".to_string(),
            nested_property(HashMap::from([
                (
                    "ontology_term".to_string(),
                    keyword_property("Ontology term", Some(64), Some(Normalizer::Lowercase)),
                ),
                (
                    "ontology_id".to_string(),
                    keyword_property("Ontology ID", Some(64), Some(Normalizer::Lowercase)),
                ),
            ])),
        ),
        (
            "source_date".to_string(),
            date_property("Date source last updated"),
        ),
        (
            "source_doc_id".to_string(),
            keyword_property(
                "Document ID containing source value",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "source_author".to_string(),
            text_property("Source publication author", None, None),
        ),
        (
            "source_year".to_string(),
            numeric_property("Source publication year", "short", None),
        ),
        (
            "source_title".to_string(),
            text_property("Source publication title", None, None),
        ),
        (
            "source_doi".to_string(),
            keyword_property(
                "Source publication DOI",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "source_pubmed_id".to_string(),
            keyword_property(
                "Source publication pubmed ID",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "source_slug".to_string(),
            keyword_property("Source url slug", Some(64), Some(Normalizer::Lowercase)),
        ),
        (
            "source_prefix".to_string(),
            keyword_property(
                "Source prefix, i.e xref in xref:value queries",
                Some(32),
                Some(Normalizer::Lowercase),
            ),
        ),
    ])
}

// Set of shared properties for for taxon, assembly, sample and feature indices
// not present in the nested "values" mapping in the taxon index
pub fn shared_non_value_properties(include_values: bool) -> HashMap<String, Property> {
    let mut properties = HashMap::from([
        (
            "key".to_string(),
            keyword_property("Attribute key", Some(64), Some(Normalizer::Lowercase)),
        ),
        (
            "source".to_string(),
            keyword_with_fields_property(
                "Source of attribute value",
                Some(64),
                Some(Normalizer::Lowercase),
                HashMap::from([("raw".to_string(), keyword_property("raw", None, None))]),
            ),
        ),
        (
            "source_url".to_string(),
            keyword_property("Source URL", Some(128), Some(Normalizer::Lowercase)),
        ),
        (
            "source_url_template".to_string(),
            keyword_property("URL template", None, None),
        ),
        (
            "metadata".to_string(),
            flattened_property("metadata associated with a value", Some(true)),
        ),
        (
            "aggregation_method".to_string(),
            keyword_property(
                "Method used to generate summary value",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "aggregation_source".to_string(),
            keyword_property(
                "Summary source (direct, ancestor, descendant)",
                Some(16),
                Some(Normalizer::Lowercase),
            ),
        ),
        ("comment".to_string(), text_property("Comment", None, None)),
        (
            "deprecated".to_string(),
            boolean_property("Flag to indicate value is deprecated"),
        ),
        (
            "deprecated_reason".to_string(),
            text_property("Reason for deprecation", None, None),
        ),
        (
            "count".to_string(),
            numeric_property("Count of individual values", "integer", None),
        ),
        (
            "min".to_string(),
            numeric_property("Minimum value (numeric types only)", "double", None),
        ),
        (
            "max".to_string(),
            numeric_property("Maximum value (numeric types only)", "double", None),
        ),
        (
            "range".to_string(),
            numeric_property("Range of values (numeric types only)", "double", None),
        ),
        (
            "total".to_string(),
            numeric_property("Total count of individual values", "double", None),
        ),
        (
            "mean".to_string(),
            numeric_property("Mean value (numeric types only)", "double", None),
        ),
        (
            "median".to_string(),
            numeric_property("Median value (numeric types only)", "double", None),
        ),
        (
            "mode".to_string(),
            numeric_property("Modal value (keyword and numeric types)", "double", None),
        ),
        (
            "stdev".to_string(),
            numeric_property("Standard deviation (numeric types only)", "float", None),
        ),
        (
            "hexbin1".to_string(),
            keyword_property(
                "H3 hexbin value (resolution 1)",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "hexbin2".to_string(),
            keyword_property(
                "H3 hexbin value (resolution 2)",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "hexbin3".to_string(),
            keyword_property(
                "H3 hexbin value (resolution 3)",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "hexbin4".to_string(),
            keyword_property(
                "H3 hexbin value (resolution 4)",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "hexbin5".to_string(),
            keyword_property(
                "H3 hexbin value (resolution 5)",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "hexbin6".to_string(),
            keyword_property(
                "H3 hexbin value (resolution 6)",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
    ]);
    // nested "values" mapping in the taxon index only
    if include_values {
        properties.extend([(
            "values".to_string(),
            nested_property(shared_value_properties()),
        )]);
    }
    properties
}

// Set of mappings for attributes in the feature index
pub fn nested_attribute_properties(include_values: bool) -> HashMap<String, Property> {
    let properties = shared_non_value_properties(include_values)
        .into_iter()
        .chain(shared_value_properties().into_iter())
        .collect();
    properties
}

// Set of mappings for identifiers in the feature index
pub fn nested_identifier_properties() -> HashMap<String, Property> {
    HashMap::from([
        (
            "identifier".to_string(),
            keyword_with_lookup_property(
                "Feature identifier",
                Some(128),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "class".to_string(),
            keyword_property(
                "Identifier class (e.g. bioproject, biosample, ...)",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "source".to_string(),
            keyword_property(
                "Source of identifier",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "source_url".to_string(),
            keyword_property("Source URL", Some(128), None),
        ),
        (
            "source_url_template".to_string(),
            keyword_property("URL template", None, None),
        ),
    ])
}

pub fn nested_lineage_properties() -> HashMap<String, Property> {
    HashMap::from([
        (
            "taxon_id".to_string(),
            keyword_property(
                "Taxon ID of ancestral taxon",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "taxon_rank".to_string(),
            keyword_property("Ancestral rank", Some(64), Some(Normalizer::Lowercase)),
        ),
        (
            "scientific_name".to_string(),
            keyword_with_fields_property(
                "Scientific name",
                Some(128),
                Some(Normalizer::Lowercase),
                HashMap::from([("raw".to_string(), keyword_property("raw", None, None))]),
            ),
        ),
        (
            "node_depth".to_string(),
            numeric_property("Cumulative branch length to ancestral taxon", "float", None),
        ),
        (
            "support_value".to_string(),
            numeric_property("Support value for node", "float", None),
        ),
    ])
}

pub fn nested_taxon_names_properties() -> HashMap<String, Property> {
    HashMap::from([
        (
            "name".to_string(),
            keyword_with_lookup_property("Taxon name", Some(128), Some(Normalizer::Lowercase)),
        ),
        (
            "class".to_string(),
            keyword_property(
                "Name class (e.g. common name, synonym, etc.)",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "source".to_string(),
            keyword_property(
                "Source DB for taxon name",
                Some(64),
                Some(Normalizer::Lowercase),
            ),
        ),
        (
            "source_url_stub".to_string(),
            keyword_property("URL slug for taxon name xref", None, None),
        ),
        (
            "source_url".to_string(),
            keyword_property("Source URL", Some(128), None),
        ),
        (
            "source_url_template".to_string(),
            keyword_property("URL template", None, None),
        ),
    ])
}
