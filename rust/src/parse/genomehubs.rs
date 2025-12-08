use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::fmt;
use std::fs::OpenOptions;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::str::FromStr;

use cpc::{eval, units::Unit};
use csv::StringRecord;

use schemars::JsonSchema;
use serde;
use serde::{Deserialize, Deserializer, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::error;
use crate::io;
use crate::parse::lookup;

use lookup::TaxonMatch;

use super::lookup::clean_name;

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub enum GHubsFileFormat {
    #[serde(rename = "csv")]
    CSV,
    #[default]
    #[serde(rename = "tsv")]
    TSV,
}

impl FromStr for GHubsFileFormat {
    type Err = ();
    fn from_str(input: &str) -> Result<GHubsFileFormat, Self::Err> {
        match input {
            "csv" => Ok(GHubsFileFormat::CSV),
            "csv.gz" => Ok(GHubsFileFormat::CSV),
            "tsv" => Ok(GHubsFileFormat::TSV),
            "tsv.gz" => Ok(GHubsFileFormat::TSV),
            _ => Err(()),
        }
    }
}

// Value may be String or Vec of Strings
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum StringOrVec {
    Single(String),
    Multiple(Vec<String>),
}

impl fmt::Display for StringOrVec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StringOrVec::Single(s) => write!(f, "{}", s),
            StringOrVec::Multiple(v) => write!(f, "{:?}", v),
        }
    }
}

// Value may be u32 or Vec of u32
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum UsizeOrVec {
    Single(usize),
    Multiple(Vec<usize>),
}

// Value may be PathBuf or Vec of PathBuf
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PathBufOrVec {
    Single(PathBuf),
    Multiple(Vec<PathBuf>),
}

// Field types
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub enum FieldType {
    #[serde(rename = "byte")]
    Byte,
    #[serde(rename = "date")]
    Date,
    #[serde(rename = "double")]
    Double,
    #[serde(rename = "float")]
    Float,
    #[serde(rename = "geo_point")]
    GeoPoint,
    #[serde(rename = "half_float")]
    HalfFloat,
    #[default]
    #[serde(rename = "keyword")]
    Keyword,
    #[serde(rename = "integer")]
    Integer,
    #[serde(rename = "long")]
    Long,
    #[serde(rename = "short")]
    Short,
    #[serde(rename = "1dp")]
    OneDP,
    #[serde(rename = "2dp")]
    TwoDP,
    #[serde(rename = "3dp")]
    ThreeDP,
    #[serde(rename = "4dp")]
    FourDP,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
pub enum SkipPartial {
    #[serde(rename = "row")]
    Row,
    #[serde(rename = "cell")]
    Cell,
}

/// GenomeHubs file configuration options
#[derive(Default, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GHubsFileConfig {
    /// Comment character
    /// Default: #
    /// This is used to skip lines in the input file
    #[serde(
        alias = "comment",
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "string_to_u8_opt"
    )]
    pub comment_char: Option<u8>,
    /// File description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    // Display group
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_group: Option<String>,
    // Display level
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_level: Option<u8>,
    // Exclusions options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusions: Option<ExclusionConfig>,
    /// File format
    /// Default: tsv
    pub format: GHubsFileFormat,
    /// Flag to indicate whether file has a header row
    pub header: bool,
    /// Filename or path relative to the configuration file
    pub name: PathBuf,
    /// Additional configuration files that must be loaded
    /// before this file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs: Option<PathBufOrVec>,
    /// Organelle type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organelle: Option<Organelle>,
    /// Source name
    #[serde(rename = "source", alias = "source_name")]
    pub source_name: Option<String>,
    /// Source abbreviation
    #[serde(
        rename = "source_abbreviation",
        alias = "abbreviation",
        skip_serializing_if = "Option::is_none"
    )]
    pub source_abbreviation: Option<String>,
    /// Source URL (Single URL for all values)
    #[serde(
        rename = "source_url",
        alias = "source_link",
        skip_serializing_if = "Option::is_none"
    )]
    pub source_url: Option<String>,
    /// Source URL stub (base URL for values)
    #[serde(rename = "source_url_stub", skip_serializing_if = "Option::is_none")]
    pub source_stub: Option<String>,
    /// Source URL suffix (suffix for values)
    #[serde(rename = "source_slug", skip_serializing_if = "Option::is_none")]
    pub source_slug: Option<String>,
    /// Source description
    #[serde(rename = "source_description", skip_serializing_if = "Option::is_none")]
    pub source_description: Option<String>,
    /// Source last updated date
    #[serde(
        rename = "source_date",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "date_format",
        default
    )]
    pub source_date: Option<String>,
    /// Source contact name
    #[serde(rename = "source_contact", skip_serializing_if = "Option::is_none")]
    pub source_contact: Option<String>,
    /// Skip partial rows or cells
    /// Default: row
    /// Options: row, cell
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_partial: Option<SkipPartial>,
    /// Relative path to a directory containing test files
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tests: Option<PathBuf>,
    /// URL to download file
    #[serde(rename = "url", skip_serializing_if = "Option::is_none")]
    pub file_url: Option<String>,
}

pub fn string_to_u8_opt<'de, D>(deserializer: D) -> Result<Option<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) if !s.is_empty() => Ok(Some(s.as_bytes()[0])),
        _ => Ok(None),
    }
}

impl GHubsFileConfig {
    pub fn get_needs(&self) -> Vec<PathBuf> {
        match &self.needs {
            Some(needs) => match needs {
                PathBufOrVec::Single(path) => vec![path.clone()],
                PathBufOrVec::Multiple(paths) => paths.clone(),
            },
            None => vec![],
        }
    }

    pub fn file_path(&self, config_path: &PathBuf, subdir: Option<&str>) -> PathBuf {
        let mut file_path = config_path.clone();

        file_path.pop();
        if let Some(subdir) = subdir {
            file_path.push(subdir);
        }
        std::fs::create_dir_all(&file_path).unwrap();
        file_path.push(&self.name);
        file_path
    }
}

/// GenomeHubs analysis configuration options
#[derive(Default, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GHubsAnalysisConfig {
    // Unique analysis ID
    pub analysis_id: String,
    // Assembly ID
    pub assembly_id: Option<StringOrVec>,
    // Taxon ID
    pub taxon_id: Option<StringOrVec>,
    // Description
    pub description: Option<String>,
    // Analysis name
    pub name: String,
    // Analysis title
    pub title: Option<String>,
}

/// GenomeHubs field constraint configuration options
#[derive(Default, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConstraintConfig {
    // List of valid values
    #[serde(
        rename = "enum",
        deserialize_with = "deserialize_to_lowercase",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub enum_values: Option<Vec<String>>,
    // Value length
    #[serde(rename = "len", skip_serializing_if = "Option::is_none")]
    pub len: Option<usize>,
    // Maximum value
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "format_number"
    )]
    pub max: Option<f64>,
    // Minimum value
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "format_number"
    )]
    pub min: Option<f64>,
}

fn deserialize_to_lowercase<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let v: Vec<String> = Vec::deserialize(deserializer)?;
    Ok(Some(v.into_iter().map(|s| s.to_lowercase()).collect()))
}

// Field types
#[derive(Default, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum FieldScale {
    #[default]
    #[serde(rename = "linear")]
    Linear,
    #[serde(rename = "log2")]
    Log2,
    #[serde(rename = "log10")]
    Log10,
    #[serde(rename = "double")]
    SQRT,
}

/// Valid summary functions
#[derive(Default, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum SummaryFunction {
    // Count
    #[serde(rename = "count")]
    Count,
    // Highest ranked value from an ordered list
    #[serde(rename = "enum")]
    Enum,
    // Length of list of values
    #[serde(rename = "length")]
    Length,
    // List of values
    #[default]
    #[serde(rename = "list")]
    List,
    // Maximum
    #[serde(rename = "max")]
    Max,
    // Mean
    #[serde(rename = "mean")]
    Mean,
    // Median
    #[serde(rename = "median")]
    Median,
    // Median with ties broken by minimum value
    #[serde(rename = "median_low")]
    MedianLow,
    // Median with ties broken by maximum value
    #[serde(rename = "median_high")]
    MedianHigh,
    // Minimum
    #[serde(rename = "min")]
    Min,
    // Mode
    #[serde(rename = "mode")]
    Mode,
    // Mode with ties broken by minimum value
    #[serde(rename = "mode_low")]
    ModeLow,
    // Mode with ties broken by maximum value
    #[serde(rename = "mode_high")]
    ModeHigh,
    // Mode with both values listed if there are two modes
    #[serde(rename = "mode_list")]
    ModeList,
    // List based on ordered priority of related list keys
    #[serde(rename = "ordered_list")]
    OrderedList,
    // No summary function
    #[serde(rename = "false")]
    None,
    // Prefer values marked as primary
    #[serde(rename = "primary")]
    Primary,
    // Standard deviation
    #[serde(rename = "std_dev")]
    StdDev,
    // Sum
    #[serde(rename = "sum")]
    Sum,
    // Variance
    #[serde(rename = "variance")]
    Variance,
}

// value may be Summary Function or Vec of Summary Functions
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SummaryFunctionOrVec {
    Single(SummaryFunction),
    Multiple(Vec<SummaryFunction>),
}

/// GenomeHubs value bins configuration options
#[derive(Default, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BinsConfig {
    // List of valid values
    pub count: u32,
    // Geographic resolution (hexagonal)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h3res: Option<u8>,
    // Maximum value
    #[serde(serialize_with = "format_number")]
    pub max: Option<f64>,
    // Minimum value
    #[serde(serialize_with = "format_number")]
    pub min: Option<f64>,
    // Value length
    pub scale: FieldScale,
}

// format numbers such that any float ending in .0 is converted to an integer
fn format_number<S>(number: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if let Some(number) = number {
        if number.fract() == 0.0 {
            serializer.serialize_i64(number.trunc() as i64)
        } else {
            serializer.serialize_f64(*number)
        }
    } else {
        serializer.serialize_none()
    }
}

/// Traverse direction values
#[derive(Default, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum TraverseDirection {
    // Both
    #[serde(rename = "both")]
    Both,
    // Down
    #[serde(rename = "down")]
    Down,
    // None
    #[serde(rename = "false")]
    None,
    // Up
    #[default]
    #[serde(rename = "up")]
    Up,
}

/// GenomeHubs exclusion configuration options.
/// Defines a list of keys to check for exclusion
/// based on the associated values
#[derive(Default, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExclusionConfig {
    // Attribute keys
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<String>>,
    // Identifier keys
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifiers: Option<Vec<String>>,
    // Taxonomy keys
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxonomy: Option<Vec<String>>,
}

/// GenomeHubs field status values
#[derive(Default, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum FieldStatus {
    // Temporary
    #[default]
    #[serde(rename = "temporary")]
    Temporary,
}

/// Valid organelle values
#[derive(Default, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum Organelle {
    // Apicoplast
    #[serde(rename = "apicoplast")]
    Apicoplast,
    // Chloroplast
    #[serde(rename = "chloroplast")]
    Chloroplast,
    // Mitochondrion
    #[serde(rename = "mitochondrion")]
    Mitochondrion,
    // Nucleus
    #[default]
    #[serde(rename = "nucleus")]
    Nucleus,
    // Plastid
    #[serde(rename = "plastid")]
    Plastid,
}

/// Value metadata configuration options
#[derive(Default, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValueMetadataConfig {
    // value description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    // value link
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    // value long description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_description: Option<String>,
}

/// GenomeHubs field configuration options
#[derive(Default, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GHubsFieldConfig {
    // Default settings for value bins
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bins: Option<BinsConfig>,
    // Free text comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    // Constraint on field values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraint: Option<ConstraintConfig>,
    // Default value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    // Field description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    // Display group
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_group: Option<String>,
    // Display level
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_level: Option<u8>,
    // Display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    // Exclusions options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusions: Option<ExclusionConfig>,
    // Function to apply to value
    #[serde(skip_serializing)]
    pub function: Option<String>,
    // Resolution of h3 grid
    // Used for geographic resolution
    // valid values range from 0 to 15
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h3res: Option<u8>,
    // Column header
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<StringOrVec>,
    // Column index
    #[serde(skip_serializing)]
    pub index: Option<UsizeOrVec>,
    // Flag to indicate value status
    // This may be represented by an integer (0 or 1) or a boolean (true or false)
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "deserialize_bool_from_int_or_bool"
    )]
    pub is_primary_value: Option<bool>,
    // String to join columns
    #[serde(skip_serializing)]
    pub join: Option<String>,
    // Attribute key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    // List key for ordered values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_key: Option<String>,
    // Long description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_description: Option<String>,
    // Additional metadata
    #[serde(skip_serializing)]
    pub metadata: Option<Box<GHubsFieldConfig>>,
    // Attribute name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    // Ordered list of keys
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<Vec<String>>,
    // Organelle type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organelle: Option<Organelle>,
    // Path to data value in raw input file
    #[serde(skip_serializing)]
    pub path: Option<String>,
    // Type to return from API if not specified
    // Default: keyword
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<SummaryFunction>,
    // Value separator
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<StringOrVec>,
    /// Source name
    #[serde(rename = "source", alias = "source_name")]
    pub source_name: Option<String>,
    /// Source abbreviation
    #[serde(
        rename = "source_abbreviation",
        alias = "abbreviation",
        skip_serializing_if = "Option::is_none"
    )]
    pub source_abbreviation: Option<String>,
    /// Source URL (Single URL for all values)
    #[serde(
        rename = "source_url",
        alias = "source_link",
        skip_serializing_if = "Option::is_none"
    )]
    pub source_url: Option<String>,
    /// Source URL stub (base URL for values)
    #[serde(rename = "source_url_stub", skip_serializing_if = "Option::is_none")]
    pub source_stub: Option<String>,
    /// Source URL suffix (suffix for values)
    #[serde(rename = "source_slug", skip_serializing_if = "Option::is_none")]
    pub source_slug: Option<String>,
    /// Source description
    #[serde(rename = "source_description", skip_serializing_if = "Option::is_none")]
    pub source_description: Option<String>,
    /// Source last updated date
    #[serde(
        rename = "source_date",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "date_format",
        default
    )]
    pub source_date: Option<String>,
    /// Source contact name
    #[serde(rename = "source_contact", skip_serializing_if = "Option::is_none")]
    pub source_contact: Option<String>,
    // Attribute status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<FieldStatus>,
    // Attribute summary functions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<SummaryFunctionOrVec>,
    // Attribute name synonyms
    #[serde(alias = "synonym", skip_serializing_if = "Option::is_none")]
    pub synonyms: Option<StringOrVec>,
    // Taxon bins
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_bins: Option<BinsConfig>,
    // Taxon display group
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_display_group: Option<String>,
    // Taxon display level
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_display_level: Option<u8>,
    // Taxon display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_display_name: Option<String>,
    // Taxon key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_key: Option<String>,
    // Taxon name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_name: Option<String>,
    // Taxon summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_summary: Option<SummaryFunctionOrVec>,
    // Taxon synonyms
    #[serde(
        rename = "taxon_synonyms",
        alias = "taxon_synonym",
        skip_serializing_if = "Option::is_none"
    )]
    pub taxon_synonyms: Option<StringOrVec>,
    // Traverse function
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_traverse: Option<SummaryFunction>,
    // Traverse direction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_traverse_direction: Option<TraverseDirection>,
    // Traverse limit is a taxon rank at which to stop filling values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_traverse_limit: Option<String>,
    // Field type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_type: Option<String>,
    // List of values to translate
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "deserialize_translate"
    )]
    pub translate: Option<HashMap<String, StringOrVec>>,
    // Traverse function
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traverse: Option<SummaryFunction>,
    // Traverse direction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traverse_direction: Option<TraverseDirection>,
    // Traverse limit is a taxon rank at which to stop filling values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traverse_limit: Option<String>,
    // Field type
    #[serde(rename = "type", default = "default_field_type")]
    pub field_type: FieldType,
    // Attribute value units
    #[serde(alias = "unit", skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    // Value metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_metadata: Option<HashMap<String, ValueMetadataConfig>>,
}

fn default_field_type() -> FieldType {
    FieldType::Keyword
}

fn deserialize_bool_from_int_or_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Bool(value) => Ok(Some(value)),
        serde_json::Value::Number(value) => Ok(Some(value.as_u64().unwrap() == 1)),
        _ => Ok(None),
    }
}

impl GHubsFieldConfig {
    fn merge(self, other: GHubsFieldConfig) -> Self {
        Self {
            bins: self.bins.or(other.bins),
            constraint: self.constraint.or(other.constraint),
            comment: self.comment.or(other.comment),
            default: self.default.or(other.default),
            description: self.description.or(other.description),
            display_group: self.display_group.or(other.display_group),
            display_level: self.display_level.or(other.display_level),
            display_name: self.display_name.or(other.display_name),
            exclusions: self.exclusions.or(other.exclusions),
            function: self.function.or(other.function),
            h3res: self.h3res.or(other.h3res),
            header: self.header.or(other.header),
            index: self.index.or(other.index),
            is_primary_value: self.is_primary_value.or(other.is_primary_value),
            join: self.join.or(other.join),
            key: self.key.or(other.key),
            list_key: self.list_key.or(other.list_key),
            long_description: self.long_description.or(other.long_description),
            metadata: self.metadata.or(other.metadata),
            name: self.name.or(other.name),
            order: self.order.or(other.order),
            organelle: self.organelle.or(other.organelle),
            path: self.path.or(other.path),
            return_type: self.return_type.or(other.return_type),
            separator: self.separator.or(other.separator),
            source_name: self.source_name.or(other.source_name),
            source_abbreviation: self.source_abbreviation.or(other.source_abbreviation),
            source_url: self.source_url.or(other.source_url),
            source_stub: self.source_stub.or(other.source_stub),
            source_slug: self.source_slug.or(other.source_slug),
            source_description: self.source_description.or(other.source_description),
            source_date: self.source_date.or(other.source_date),
            source_contact: self.source_contact.or(other.source_contact),
            status: self.status.or(other.status),
            summary: self.summary.or(other.summary),
            synonyms: self.synonyms.or(other.synonyms),
            taxon_bins: self.taxon_bins.or(other.taxon_bins),
            taxon_display_group: self.taxon_display_group.or(other.taxon_display_group),
            taxon_display_level: self.taxon_display_level.or(other.taxon_display_level),
            taxon_display_name: self.taxon_display_name.or(other.taxon_display_name),
            taxon_key: self.taxon_key.or(other.taxon_key),
            taxon_name: self.taxon_name.or(other.taxon_name),
            taxon_summary: self.taxon_summary.or(other.taxon_summary),
            taxon_synonyms: self.taxon_synonyms.or(other.taxon_synonyms),
            taxon_traverse: self.taxon_traverse.or(other.taxon_traverse),
            taxon_traverse_direction: self
                .taxon_traverse_direction
                .or(other.taxon_traverse_direction),
            taxon_traverse_limit: self.taxon_traverse_limit.or(other.taxon_traverse_limit),
            taxon_type: self.taxon_type.or(other.taxon_type),
            translate: self.translate.or(other.translate),
            traverse: self.traverse.or(other.traverse),
            traverse_direction: self.traverse_direction.or(other.traverse_direction),
            traverse_limit: self.traverse_limit.or(other.traverse_limit),
            field_type: self.field_type,
            units: self.units.or(other.units),
            value_metadata: self.value_metadata.or(other.value_metadata),
        }
    }
}

/// Merges 2 GenomeHubs configuration files
fn merge_attributes(
    self_attributes: Option<HashMap<String, GHubsFieldConfig>>,
    other_attributes: Option<HashMap<String, GHubsFieldConfig>>,
    merged_attributes: &mut HashMap<String, GHubsFieldConfig>,
) {
    if let Some(attributes) = self_attributes {
        if other_attributes.is_some() {
            let new_attributes = other_attributes.unwrap();
            for (field, other_config) in new_attributes.clone() {
                if let Some(config) = attributes.get(&field) {
                    merged_attributes.insert(field.clone(), config.clone().merge(other_config));
                } else {
                    merged_attributes.insert(field.clone(), other_config.clone());
                }
            }
            for (field, config) in attributes {
                if let Some(_) = new_attributes.get(&field) {
                    continue;
                } else {
                    merged_attributes.insert(field.clone(), config.clone());
                }
            }
        } else {
            for (field, config) in attributes {
                merged_attributes.insert(field.clone(), config.clone());
            }
        }
    } else if let Some(attributes) = other_attributes {
        for (field, config) in attributes {
            merged_attributes.insert(field.clone(), config.clone());
        }
    }
}

/// GenomeHubs configuration options
#[derive(Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GHubsConfig {
    /// File configuration options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<GHubsFileConfig>,
    /// Analysis configuration options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<GHubsAnalysisConfig>,
    /// Attribute defaults
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defaults: Option<GHubsDefaultsConfig>,
    /// Attribute fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<HashMap<String, GHubsFieldConfig>>,
    /// Identifier fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifiers: Option<HashMap<String, GHubsFieldConfig>>,
    /// Metadata fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, GHubsFieldConfig>>,
    /// Taxon names
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_names: Option<HashMap<String, GHubsFieldConfig>>,
    /// Taxonomy fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxonomy: Option<HashMap<String, GHubsFieldConfig>>,
    /// Feature fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<HashMap<String, GHubsFieldConfig>>,
    /// Path to config file
    #[serde(skip)]
    pub file_path: PathBuf,

    /// Validation counts
    #[serde(skip)]
    pub validation_counts: ValidationCounts,

    /// CSV reader
    #[serde(skip)]
    pub csv_reader: Option<csv::Reader<Box<dyn BufRead>>>,
    /// CSV writer
    #[serde(skip)]
    pub csv_writer: Option<csv::Writer<Box<dyn Write>>>,
    /// Exception writer
    /// JSONL writer for exceptions
    #[serde(skip)]
    pub exception_writer: Option<std::fs::File>,
    /// List of output headers
    /// Used to write validated records
    /// to CSV/TSV file
    /// Set when first record is read
    /// and used to write headers
    /// to output file
    #[serde(skip)]
    pub output_headers: Vec<(String, String)>,
}

/// GenomeHubs configuration options
#[derive(Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GHubsDefaultsConfig {
    /// File configuration options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<GHubsFileConfig>,
    /// Analysis configuration options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<GHubsAnalysisConfig>,
    /// Attribute configuration options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<GHubsFieldConfig>,
    /// Identifier configuration options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifiers: Option<GHubsFieldConfig>,
}

impl GHubsConfig {
    pub fn new(config_file: &PathBuf) -> Result<GHubsConfig, error::Error> {
        let ghubs_config = parse_genomehubs_config(config_file)?;

        Ok(ghubs_config)
    }

    pub fn get(&self, key: &str) -> Option<&HashMap<String, GHubsFieldConfig>> {
        match key {
            "attributes" => self.attributes.as_ref(),
            "taxonomy" => self.taxonomy.as_ref(),
            "taxon_names" => self.taxon_names.as_ref(),
            _ => None,
        }
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut HashMap<String, GHubsFieldConfig>> {
        match key {
            "attributes" => self.attributes.as_mut(),
            "taxonomy" => self.taxonomy.as_mut(),
            "taxon_names" => self.taxon_names.as_mut(),
            _ => None,
        }
    }
    fn merge(self, other: GHubsConfig) -> Self {
        let mut merged_attributes = HashMap::new();
        let self_attributes = self.attributes;
        let other_attributes = other.attributes;
        merge_attributes(self_attributes, other_attributes, &mut merged_attributes);
        let mut merged_taxonomy = HashMap::new();
        let self_taxonomy = self.taxonomy;
        let other_taxonomy = other.taxonomy;
        merge_attributes(self_taxonomy, other_taxonomy, &mut merged_taxonomy);
        let mut merged_taxon_names = HashMap::new();
        let self_taxon_names = self.taxon_names;
        let other_taxon_names = other.taxon_names;
        merge_attributes(self_taxon_names, other_taxon_names, &mut merged_taxon_names);
        Self {
            file: self.file.or(other.file),
            attributes: Some(merged_attributes),
            taxonomy: Some(merged_taxonomy),
            taxon_names: Some(merged_taxon_names),
            file_path: self.file_path,
            ..Default::default()
        }
    }

    pub fn update_config(&mut self, key: &str, headers: &StringRecord) {
        for (_, field) in self.borrow_mut().get_mut(key).unwrap().iter_mut() {
            if field.header.is_some() {
                // if let Some(header) = &field.header {
                // let field_idx = &mut field.index;
                field.index = match &field.header.as_ref().unwrap().clone() {
                    StringOrVec::Single(item) => Some(UsizeOrVec::Single(
                        key_index(headers, item.as_str()).unwrap(),
                    )),
                    StringOrVec::Multiple(list) => Some(UsizeOrVec::Multiple(
                        list.iter()
                            .map(|item| key_index(headers, item.as_str()).unwrap())
                            .collect::<Vec<usize>>(),
                    )),
                };
                // field.index = field_index;
            };
        }
    }

    pub fn to_yaml(&self) -> Result<String, error::Error> {
        let yaml = serde_yaml::to_string(&self)?;
        Ok(yaml)
    }

    pub fn write_yaml(&self, output_file: &PathBuf) -> Result<(), error::Error> {
        let yaml = self.to_yaml()?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(output_file)?;
        file.write_all(yaml.as_bytes())?;
        Ok(())
    }

    pub fn init_csv_reader(
        &mut self,
        keys: Option<Vec<&str>>,
        skip_file: bool,
    ) -> Result<csv::Reader<Box<dyn BufRead>>, error::Error> {
        if self.file.is_none() || skip_file {
            // return an empty reader
            return Ok(csv::Reader::from_reader(io::get_empty_reader()));
        }
        let file_config = self.file.clone().unwrap();
        let config_path = self.file_path.clone();
        let file_path = file_config.file_path(&config_path, None);
        let delimiter = match file_config.format {
            GHubsFileFormat::CSV => b',',
            GHubsFileFormat::TSV => b'\t',
        };
        if !file_path.exists() {
            return Err(error::Error::FileNotFound(format!("{}", file_path.display())).into());
        }
        let mut rdr = io::get_csv_reader(
            &Some(file_path.clone()),
            delimiter,
            file_config.header,
            file_config.comment_char,
            0,
            false,
        );

        if let Some(keys) = keys {
            if file_config.header {
                let headers = rdr.headers().unwrap().clone();
                for key in keys.iter() {
                    if self.get(key).is_some() {
                        self.update_config(key, &headers);
                    }
                }
            }
        }
        Ok(rdr)
    }

    pub fn init_file_writers(&mut self, write_validated: bool, write_exceptions: bool) -> () {
        if self.file.is_none() {
            return;
        }
        let file_config = self.file.clone().unwrap();
        let config_path = self.file_path.clone();
        let delimiter = match file_config.format {
            GHubsFileFormat::CSV => b',',
            GHubsFileFormat::TSV => b'\t',
        };
        let writer = if write_validated {
            Some(io::get_csv_writer(
                &Some(file_config.file_path(&config_path, Some("validated"))),
                delimiter,
            ))
        } else {
            None
        };
        self.csv_writer = writer;

        // set up file to write exceptions as jsonl in exceptions subdirectory
        let exception_writer = if write_exceptions {
            let mut exception_path = config_path.clone();
            exception_path.pop();
            exception_path.push("exceptions");
            std::fs::create_dir_all(&exception_path).unwrap();
            exception_path.push("exceptions.jsonl");
            if exception_path.exists() {
                std::fs::remove_file(&exception_path).unwrap();
            }
            let writer = OpenOptions::new()
                .append(true)
                .create(true)
                .open(exception_path)
                .unwrap();
            Some(writer)
        } else {
            None
        };
        self.exception_writer = exception_writer;
    }

    pub fn init_taxon_id(&mut self) {
        let taxonomy = self.get_mut("taxonomy").unwrap();
        if !taxonomy.contains_key("taxon_id") {
            let taxon_id_config = GHubsFieldConfig {
                field_type: FieldType::Keyword,
                header: Some(StringOrVec::Single("taxon_id".to_string())),
                ..Default::default()
            };
            taxonomy.insert("taxon_id".to_string(), taxon_id_config);
        }
    }

    pub fn init_taxon_names(&mut self) -> HashMap<String, HashMap<String, String>> {
        let file_config = self.file.clone().unwrap();
        let config_path = self.file_path.clone();
        let file_path = file_config.file_path(&config_path, Some("names"));
        let mut fixed_names = HashMap::new();
        if !file_path.exists() {
            return fixed_names;
        }
        let delimiter = match file_config.format {
            GHubsFileFormat::CSV => b',',
            GHubsFileFormat::TSV => b'\t',
        };
        let mut rdr = io::get_csv_reader(
            &Some(file_path),
            delimiter,
            true,
            file_config.comment_char,
            0,
            false,
        );
        let expected_headers = vec!["taxon_id", "input", "rank"];
        let headers = rdr.headers().unwrap().clone();
        for (i, header) in headers.iter().enumerate() {
            if header != expected_headers[i] {
                panic!("Invalid header: {}", header);
            }
        }
        for result in rdr.records() {
            let record = result.unwrap();
            let taxon_id = record.get(0).unwrap().to_string();
            let name = record.get(1).unwrap().to_string();
            let rank = record.get(2).unwrap().to_string();
            let at_rank = fixed_names.entry(rank).or_insert(HashMap::new());
            at_rank.insert(clean_name(&name), taxon_id);
        }
        fixed_names
    }

    pub fn write_processed_row(
        &mut self,
        processed: &HashMap<String, HashMap<String, String>>,
    ) -> Result<(), error::Error> {
        if self.csv_writer.is_none() {
            return Ok(());
        }
        let writer;

        if self.output_headers.is_empty() {
            for key in processed.keys() {
                let fields: Vec<String> = self.get(key).unwrap().keys().cloned().collect();
                for field in fields {
                    self.output_headers.push((key.clone(), field));
                }
            }
            writer = self.csv_writer.as_mut().unwrap();
            writer.write_record(self.output_headers.iter().map(|(_, field)| field))?;
        } else {
            writer = self.csv_writer.as_mut().unwrap();
        }

        let mut row = vec![];
        for (key, field) in self.output_headers.iter() {
            if let Some(nested) = processed.get(key) {
                if let Some(value) = nested.get(field) {
                    row.push(value.clone());
                } else {
                    row.push("None".to_string());
                }
            }
        }
        writer.write_record(&row)?;
        Ok(())
    }

    pub fn write_modified_row(
        &mut self,
        processed: &HashMap<String, HashMap<String, String>>,
        key: &str,
        field: String,
        value: String,
    ) -> Result<(), error::Error> {
        let mut updated = processed.clone();
        updated.get_mut(key).unwrap().insert(field, value);
        self.write_processed_row(&updated)
    }

    pub fn handle_error(&mut self, error: &error::Error, row_index: usize) {
        let report = ValidationReport {
            row_index,
            counts: ValidationCounts {
                errors: 1,
                total: 1,
                ..Default::default()
            },
            status: ValidationStatus::Error,
            errors: vec![format!("Error reading record: {}", error)],
            ..Default::default()
        };
        self.write_exception(&report);
        self.validation_counts.errors += 1;
    }

    pub fn write_exception(&mut self, report: &ValidationReport) {
        self.exception_writer.as_mut().map(|writer| {
            writer.write_all(report.to_jsonl().as_bytes()).unwrap();
            writer.write_all(b"\n").unwrap();
        });
    }

    pub fn validate_values(&mut self, key: &str, record: &StringRecord) -> ValidationReport {
        let mut validated = HashMap::new();
        let mut invalid: HashMap<String, Vec<String>> = HashMap::new();
        let mut partial: HashMap<String, Vec<String>> = HashMap::new();
        let blank: Vec<String> = vec![];
        let mut field_counts = ValidationCounts::default();
        let skip_partial = self.file.as_ref().unwrap().skip_partial.clone();

        for (field_name, field) in self.borrow_mut().get_mut(key).unwrap().iter_mut() {
            if let Some(index) = &field.index {
                let string_value = match index {
                    UsizeOrVec::Single(idx) => record.get(idx.to_owned()).unwrap().to_string(),
                    UsizeOrVec::Multiple(indices) => indices
                        .iter()
                        .map(|idx| record.get(idx.to_owned()).unwrap_or(""))
                        .collect::<Vec<&str>>()
                        .join(&field.join.as_ref().unwrap_or(&"".to_string())),
                };
                let (values, invalid_values, status) = process_value(string_value, field).unwrap();
                field_counts.total += 1;
                let is_valid = match status {
                    ValidationStatus::Valid => true,
                    ValidationStatus::Blank => true,
                    _ => false,
                };
                match status {
                    ValidationStatus::Valid => field_counts.valid += 1,
                    ValidationStatus::Invalid => {
                        field_counts.invalid += 1;
                        invalid.insert(field_name.clone(), invalid_values);
                    }
                    ValidationStatus::Partial => {
                        field_counts.partial += 1;
                        partial.insert(field_name.clone(), invalid_values);
                    }
                    ValidationStatus::Blank => {
                        field_counts.blank += 1;
                        field_counts.valid += 1;
                    }
                    ValidationStatus::Error => {
                        field_counts.errors += 1;
                        field_counts.invalid += 1;
                        invalid.insert(field_name.clone(), invalid_values);
                    }
                    ValidationStatus::None => {
                        field_counts.total -= 1;
                    }
                    _ => {}
                }
                let mut validated_value: String = values
                    .iter()
                    .map(|(v, _)| v.clone())
                    .collect::<Vec<String>>()
                    .join(";");
                if !is_valid {
                    if let Some(skip) = skip_partial.clone() {
                        if skip == SkipPartial::Cell {
                            validated_value = "None".to_string();
                        }
                    }
                }
                validated.insert(field_name.clone(), validated_value);
            }
        }
        let status = {
            if field_counts.valid == field_counts.total {
                ValidationStatus::Valid
            } else if field_counts.valid > 0 {
                ValidationStatus::Partial
            } else if field_counts.blank == field_counts.total {
                ValidationStatus::Blank
            } else {
                ValidationStatus::Invalid
            }
        };
        let report = ValidationReport {
            row_index: 0,
            status,
            counts: field_counts,
            invalid,
            partial,
            blank,
            validated,
            ..Default::default()
        };
        report
    }

    pub fn validate_record(
        &mut self,
        record: &StringRecord,
        row_index: usize,
        keys: &Vec<&str>,
    ) -> (HashMap<String, HashMap<String, String>>, ValidationReport) {
        let mut processed = HashMap::new();
        let mut combined_report = ValidationReport {
            row_index,
            ..Default::default()
        };
        for key in keys.iter() {
            if self.get(key).is_some() {
                let report = self.validate_values(key, &record);
                let validated = report.validated.clone();
                combined_report.combine_reports(report);
                processed.insert(key.to_string(), validated);
            }
        }
        self.validation_counts.total += 1;

        match combined_report.status {
            ValidationStatus::Valid => self.validation_counts.valid += 1,
            ValidationStatus::Invalid => self.validation_counts.invalid += 1,
            ValidationStatus::Partial => self.validation_counts.partial += 1,
            ValidationStatus::Blank => self.validation_counts.blank += 1,
            ValidationStatus::Error => self.validation_counts.errors += 1,
            _ => {}
        }

        if combined_report.status != ValidationStatus::Valid {
            self.write_exception(&combined_report);
        }
        (processed, combined_report)
    }
}

/// GenomeHubs source options
#[derive(Default, Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct Source {
    /// Source name
    #[serde(rename = "source")]
    pub source: String,
    /// Source abbreviation
    #[serde(
        rename = "source_abbreviation",
        alias = "abbreviation",
        skip_serializing_if = "Option::is_none"
    )]
    pub abbreviation: Option<String>,
    /// Source URL (Single URL for all values)
    #[serde(
        rename = "source_url",
        alias = "source_link",
        skip_serializing_if = "Option::is_none"
    )]
    pub url: Option<String>,
    /// Source URL stub (base URL for values)
    #[serde(rename = "source_url_stub", skip_serializing_if = "Option::is_none")]
    pub stub: Option<String>,
    /// Source URL suffix (suffix for values)
    #[serde(rename = "source_slug", skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    /// Source description
    #[serde(rename = "source_description", skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Source last updated date
    #[serde(
        rename = "source_date",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "date_format",
        default
    )]
    pub date: Option<String>,
    /// Source contact name
    #[serde(rename = "source_contact", skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
}

// enforce YYYY-MM-DD date format or rasie error
fn date_format<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    if let Some(ref s) = opt {
        if chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok() {
            Ok(Some(s.clone()))
        } else {
            Err(serde::de::Error::custom(format!(
                "Invalid date format `{}`. Dates must be `YYYY-MM-DD`",
                s
            )))
        }
    } else {
        Ok(None)
    }
}

impl Source {
    pub fn new(config: &GHubsConfig) -> Source {
        if let Some(_file_config) = config.file.clone() {
            // let name = file_config.source.file_stem().unwrap().to_str().unwrap();
            // let abbreviation = name.to_case(Case::Upper);
            // Source {
            //     name: name.to_string(),
            //     abbreviation,
            //     ..Default::default()
            // }
            Source {
                ..Default::default()
            }
        } else {
            Source {
                ..Default::default()
            }
        }
    }
}

// Parse a GenomeHubs configuration file
fn parse_genomehubs_config(config_file: &PathBuf) -> Result<GHubsConfig, error::Error> {
    let reader = match io::file_reader(config_file.clone()) {
        Ok(r) => r,
        Err(_) => {
            return Err(error::Error::FileNotFound(format!(
                "{}",
                &config_file.to_str().unwrap()
            )))
        }
    };
    let mut ghubs_config: GHubsConfig = match serde_yaml::from_reader(reader) {
        Ok(options) => options,
        Err(err) => {
            return Err(error::Error::SerdeError(format!(
                "{} {}",
                &config_file.to_str().unwrap(),
                err.to_string()
            )))
        }
    };
    ghubs_config.file_path = config_file.clone();
    if let Some(file_config) = &ghubs_config.file {
        if let Some(needs) = &file_config.needs {
            let mut base_path = config_file.clone();
            base_path.pop();
            let needs_files = match needs {
                PathBufOrVec::Single(file) => {
                    base_path.push(file);
                    vec![base_path]
                }
                PathBufOrVec::Multiple(files) => {
                    let mut needs_paths = vec![];
                    for file in files.iter() {
                        let mut needs_path = base_path.clone();
                        needs_path.push(file);
                        needs_paths.push(needs_path);
                    }
                    needs_paths
                }
            };
            for needs_file in needs_files.iter() {
                let extra_config = match parse_genomehubs_config(&needs_file) {
                    Ok(extra_config) => extra_config,
                    Err(err) => return Err(err),
                };
                // TODO: combine_configs(extra_config, ghubs_config);
                ghubs_config = extra_config.merge(ghubs_config);
            }
        }
    }
    ghubs_config.apply_defaults();
    Ok(ghubs_config)
}

fn key_index(headers: &StringRecord, key: &str) -> Result<usize, error::Error> {
    match headers.iter().position(|column| column == key) {
        Some(index) => Ok(index),
        None => Err(error::Error::IndexError(format!(
            "Column '{}' does not exist.",
            key
        ))),
    }
}

fn check_bounds<T: Into<f64> + Copy>(value: &T, constraint: &ConstraintConfig) -> bool {
    let val: f64 = Into::<f64>::into(value.to_owned());
    if let Some(min) = constraint.min {
        if val < min {
            eprintln!("Value {} is less than minimum {}", val, min);
            return false;
        }
    }
    if let Some(max) = constraint.max {
        if val > max {
            eprintln!("Value {} is greater than maximum {}", val, max);
            return false;
        }
    }
    if let Some(len) = constraint.len {
        if val.to_string().len() > len {
            eprintln!("Value {} is longer than {}", val, len);
            return false;
        }
    }
    if let Some(enum_values) = &constraint.enum_values {
        if !enum_values.contains(&val.to_string().to_lowercase()) {
            // eprintln!("Value {} is not in {:?}", val, enum_values);
            return false;
        }
    }
    true
}

fn check_string_bounds(value: &String, constraint: &ConstraintConfig) -> bool {
    if let Some(len) = constraint.len {
        if value.len() > len {
            eprintln!("Value {} is longer than {}", value, len);
            return false;
        }
    }
    if let Some(enum_values) = &constraint.enum_values {
        if !enum_values.contains(&value.to_lowercase()) {
            // eprintln!("Value {} is not in {:?}", value, enum_values);
            return false;
        }
    }
    true
}

// fn apply_constraint(value: &mut GHubsConfig, constraint: &ConstraintConfig) {}

fn validate_double(value: &String, constraint: &ConstraintConfig) -> Result<bool, error::Error> {
    let v = value
        .parse::<f64>()
        .map_err(|_| error::Error::ParseError(format!("Invalid double value: {}", value)))?;
    Ok(check_bounds(&v, constraint))
}

fn apply_validation(value: String, field: &GHubsFieldConfig) -> Result<bool, error::Error> {
    let constraint = match field.constraint.to_owned() {
        Some(c) => c,
        None => ConstraintConfig {
            ..Default::default()
        },
    };
    let ref field_type = field.field_type;
    let valid = match field_type {
        FieldType::Byte => {
            let dot_pos = value.find(".").unwrap_or(value.len());
            let v = value[..dot_pos]
                .parse::<i8>()
                .map_err(|_| error::Error::ParseError(format!("Invalid byte value: {}", value)))?;
            check_bounds(&v, &constraint)
        }
        FieldType::Date => true,
        FieldType::Double => validate_double(&value, &constraint)?,

        FieldType::Float => {
            let v = value
                .parse::<f32>()
                .map_err(|_| error::Error::ParseError(format!("Invalid float value: {}", value)))?;
            check_bounds(&v, &constraint)
        }
        FieldType::GeoPoint => true,
        FieldType::HalfFloat => {
            let v = value.parse::<f32>().map_err(|_| {
                error::Error::ParseError(format!("Invalid half_float value: {}", value))
            })?;
            check_bounds(&v, &constraint)
        }
        FieldType::Keyword => {
            let v = value.parse::<String>().map_err(|_| {
                error::Error::ParseError(format!("Invalid keyword value: {}", value))
            })?;
            check_string_bounds(&v, &constraint)
        }
        FieldType::Integer => {
            let dot_pos = value.find(".").unwrap_or(value.len());
            let v = value[..dot_pos].parse::<i32>().map_err(|_| {
                error::Error::ParseError(format!("Invalid integer value: {}", value))
            })?;
            check_bounds(&v, &constraint)
        }
        FieldType::Long => {
            let dot_pos = value.find(".").unwrap_or(value.len());
            value[..dot_pos]
                .parse::<i64>()
                .map_err(|_| error::Error::ParseError(format!("Invalid long value: {}", value)))?;
            validate_double(&value, &constraint)?
        }
        FieldType::Short => {
            let dot_pos = value.find(".").unwrap_or(value.len());
            let v = value[..dot_pos]
                .parse::<i16>()
                .map_err(|_| error::Error::ParseError(format!("Invalid short value: {}", value)))?;
            check_bounds(&v, &constraint)
        }
        FieldType::OneDP => true,
        FieldType::TwoDP => true,
        FieldType::ThreeDP => true,
        FieldType::FourDP => true,
    };
    Ok(valid)
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ValidationStatus {
    Valid,
    Invalid,
    Partial,
    Blank,
    Error,
    #[default]
    None,
    Spellcheck,
    Putative,
    Mismatch,
    Multimatch,
    Nomatch,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ValidationCounts {
    pub total: usize,
    pub valid: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub invalid: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub partial: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub blank: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub errors: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub spellcheck: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub putative: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub mismatch: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub multimatch: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub nomatch: usize,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

impl ValidationCounts {
    pub fn to_json(&self) -> String {
        // summarise as json
        serde_json::to_string_pretty(&self).unwrap()
    }

    pub fn to_jsonl(&self) -> String {
        // summarise as jsonl
        serde_json::to_string(&self).unwrap()
    }

    pub fn update(&mut self, other: &ValidationCounts) {
        if other.total >= 1 {
            self.total += 1
        };
        if other.valid >= 1 {
            self.valid += 1
        };
        if other.invalid >= 1 {
            self.invalid += 1
        };
        if other.partial >= 1 {
            self.partial += 1
        };
        if other.blank >= 1 {
            self.blank += 1
        };
        if other.errors >= 1 {
            self.errors += 1
        };
        if other.spellcheck >= 1 {
            self.spellcheck += 1
        };
        if other.putative >= 1 {
            self.putative += 1
        };
        if other.mismatch >= 1 {
            self.mismatch += 1
        };
        if other.multimatch >= 1 {
            self.multimatch += 1
        };
        if other.nomatch >= 1 {
            self.nomatch += 1
        };
    }
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct ValidationReport {
    pub row_index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_name: Option<String>,
    pub status: ValidationStatus,
    pub counts: ValidationCounts,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub invalid: HashMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub partial: HashMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blank: Vec<String>,
    #[serde(skip_serializing)]
    pub validated: HashMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spellcheck: Vec<TaxonMatch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub putative: Vec<TaxonMatch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mismatch: Vec<TaxonMatch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub multimatch: Vec<TaxonMatch>,
}

impl ValidationReport {
    pub fn to_json(&self) -> String {
        // summarise as json
        serde_json::to_string_pretty(&self).unwrap()
    }

    pub fn to_jsonl(&self) -> String {
        // summarise as jsonl
        serde_json::to_string(&self).unwrap()
    }

    pub fn combine_reports(&mut self, other: ValidationReport) {
        self.status = match other.status {
            ValidationStatus::Partial => ValidationStatus::Partial,
            ValidationStatus::Error => ValidationStatus::Error,
            _ => {
                if self.status == other.status {
                    self.status.clone()
                } else if self.status == ValidationStatus::None {
                    other.status
                } else if self.status == ValidationStatus::Valid
                    && other.status == ValidationStatus::Invalid
                {
                    ValidationStatus::Partial
                } else if self.status == ValidationStatus::Invalid
                    && other.status == ValidationStatus::Valid
                {
                    ValidationStatus::Partial
                } else {
                    self.status.clone()
                }
            }
        };
        self.counts.valid += other.counts.valid;
        self.counts.invalid += other.counts.invalid;
        self.counts.partial += other.counts.partial;
        self.counts.blank += other.counts.blank;
        self.counts.errors += other.counts.errors;
        self.counts.total += other.counts.total;
        self.invalid.extend(other.invalid);
        self.partial.extend(other.partial);
        self.blank.extend(other.blank);
        self.validated.extend(other.validated);
    }
}

fn apply_function(value: String, field: &GHubsFieldConfig) -> (String, ValidationStatus) {
    if value == "" || value == "None" || value == "NA" {
        return ("None".to_string(), ValidationStatus::Blank);
    }
    let mut val = value;
    if let Some(ref function) = field.function {
        let equation = function.replace("{}", val.as_str());
        let value = eval(equation.as_str(), false, Unit::NoUnit, false).unwrap();
        val = format!("{}", value);
    }
    match apply_validation(val.clone(), &field) {
        Ok(is_valid) => {
            if is_valid {
                (val, ValidationStatus::Valid)
            } else {
                ("None".to_string(), ValidationStatus::Invalid)
            }
        }
        Err(_) => ("None".to_string(), ValidationStatus::Error),
    }
}

fn normalize_key(s: &str) -> String {
    s.trim().to_lowercase().nfc().collect::<String>()
}

fn deserialize_translate<'de, D>(
    deserializer: D,
) -> Result<Option<HashMap<String, StringOrVec>>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<HashMap<String, StringOrVec>>::deserialize(deserializer)?;
    if let Some(map) = opt {
        let normalized = map
            .into_iter()
            .map(|(k, v)| (normalize_key(&k), v))
            .collect();
        Ok(Some(normalized))
    } else {
        Ok(None)
    }
}

// Process a value: translate, apply function, validate, and return results
fn process_value(
    value: String,
    field: &GHubsFieldConfig,
) -> Result<(Vec<(String, String)>, Vec<String>, ValidationStatus), error::Error> {
    use unicode_normalization::UnicodeNormalization;
    let mut values = vec![];
    let mut invalid_values = vec![];
    let mut status = ValidationStatus::None;
    // Use field separator if present, otherwise default to ';'
    let sep = field
        .separator
        .as_ref()
        .map(|s| match s {
            StringOrVec::Single(sep) => sep.as_str(),
            StringOrVec::Multiple(vec) => vec.get(0).map(|s| s.as_str()).unwrap_or(";"),
        })
        .unwrap_or(";");
    let mut input_values: Vec<String> = value
        .split(sep)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "None" && s != "NA")
        .collect();
    if input_values.is_empty() {
        return Ok((vec![], vec![], ValidationStatus::Blank));
    }
    // Translation
    if let Some(translate) = &field.translate {
        let mut translated = vec![];
        for v in input_values.iter() {
            let norm_v = v.trim().to_lowercase().nfc().collect::<String>();
            if let Some(t) = translate.get(&norm_v) {
                match t {
                    StringOrVec::Single(s) => {
                        if !s.trim().is_empty() && s.trim().to_lowercase() != "none" {
                            translated.push(s.clone());
                        }
                    }
                    StringOrVec::Multiple(vec) => {
                        for s in vec {
                            if !s.trim().is_empty() && s.trim().to_lowercase() != "none" {
                                translated.push(s.clone());
                            }
                        }
                    }
                }
            } else if !v.trim().is_empty() && v.trim().to_lowercase() != "none" {
                translated.push(v.clone());
            }
        }
        input_values = translated;
    }
    // Apply function and validate
    for v in input_values.iter() {
        let (val, val_status) = apply_function(v.clone(), field);
        if val_status == ValidationStatus::Valid {
            values.push((val.clone(), v.clone()));
        } else {
            invalid_values.push(v.clone());
        }
        status = match (status.clone(), val_status) {
            (ValidationStatus::None, s) => s,
            (ValidationStatus::Valid, ValidationStatus::Valid) => ValidationStatus::Valid,
            (ValidationStatus::Valid, ValidationStatus::Invalid) => ValidationStatus::Partial,
            (ValidationStatus::Partial, _) => ValidationStatus::Partial,
            (_, ValidationStatus::Partial) => ValidationStatus::Partial,
            (_, ValidationStatus::Blank) => status,
            (_, ValidationStatus::Error) => ValidationStatus::Error,
            (s, _) => s,
        };
    }
    if values.is_empty() && !invalid_values.is_empty() {
        status = ValidationStatus::Invalid;
    }
    Ok((values, invalid_values, status))
}

// --- Default propagation helpers for config merging ---
impl GHubsFileConfig {
    pub fn merge_missing(&mut self, other: &GHubsFileConfig) {
        if self.comment_char.is_none() {
            self.comment_char = other.comment_char;
        }
        if self.description.is_none() {
            self.description = other.description.clone();
        }
        if self.display_group.is_none() {
            self.display_group = other.display_group.clone();
        }
        if self.display_level.is_none() {
            self.display_level = other.display_level;
        }
        if self.exclusions.is_none() {
            self.exclusions = other.exclusions.clone();
        }
        if self.organelle.is_none() {
            self.organelle = other.organelle.clone();
        }
        if self.source_name.is_none() {
            self.source_name = other.source_name.clone();
        }
        if self.source_abbreviation.is_none() {
            self.source_abbreviation = other.source_abbreviation.clone();
        }
        if self.source_url.is_none() {
            self.source_url = other.source_url.clone();
        }
        if self.source_stub.is_none() {
            self.source_stub = other.source_stub.clone();
        }
        if self.source_slug.is_none() {
            self.source_slug = other.source_slug.clone();
        }
        if self.source_description.is_none() {
            self.source_description = other.source_description.clone();
        }
        if self.source_date.is_none() {
            self.source_date = other.source_date.clone();
        }
        if self.source_contact.is_none() {
            self.source_contact = other.source_contact.clone();
        }
        if self.skip_partial.is_none() {
            self.skip_partial = other.skip_partial.clone();
        }
        if self.tests.is_none() {
            self.tests = other.tests.clone();
        }
        if self.file_url.is_none() {
            self.file_url = other.file_url.clone();
        }
    }
}

impl GHubsFieldConfig {
    pub fn merge_missing(&mut self, other: &GHubsFieldConfig) {
        if self.bins.is_none() {
            self.bins = other.bins.clone();
        }
        if self.comment.is_none() {
            self.comment = other.comment.clone();
        }
        if self.constraint.is_none() {
            self.constraint = other.constraint.clone();
        }
        if self.default.is_none() {
            self.default = other.default.clone();
        }
        if self.description.is_none() {
            self.description = other.description.clone();
        }
        if self.display_group.is_none() {
            self.display_group = other.display_group.clone();
        }
        if self.display_level.is_none() {
            self.display_level = other.display_level;
        }
        if self.display_name.is_none() {
            self.display_name = other.display_name.clone();
        }
        if self.exclusions.is_none() {
            self.exclusions = other.exclusions.clone();
        }
        if self.function.is_none() {
            self.function = other.function.clone();
        }
        if self.h3res.is_none() {
            self.h3res = other.h3res;
        }
        if self.header.is_none() {
            self.header = other.header.clone();
        }
        if self.index.is_none() {
            self.index = other.index.clone();
        }
        if self.is_primary_value.is_none() {
            self.is_primary_value = other.is_primary_value;
        }
        if self.join.is_none() {
            self.join = other.join.clone();
        }
        if self.key.is_none() {
            self.key = other.key.clone();
        }
        if self.list_key.is_none() {
            self.list_key = other.list_key.clone();
        }
        if self.long_description.is_none() {
            self.long_description = other.long_description.clone();
        }
        if self.metadata.is_none() {
            self.metadata = other.metadata.clone();
        }
        if self.name.is_none() {
            self.name = other.name.clone();
        }
        if self.order.is_none() {
            self.order = other.order.clone();
        }
        if self.organelle.is_none() {
            self.organelle = other.organelle.clone();
        }
        if self.path.is_none() {
            self.path = other.path.clone();
        }
        if self.return_type.is_none() {
            self.return_type = other.return_type.clone();
        }
        if self.separator.is_none() {
            self.separator = other.separator.clone();
        }
        if self.source_name.is_none() {
            self.source_name = other.source_name.clone();
        }
        if self.source_abbreviation.is_none() {
            self.source_abbreviation = other.source_abbreviation.clone();
        }
        if self.source_url.is_none() {
            self.source_url = other.source_url.clone();
        }
        if self.source_stub.is_none() {
            self.source_stub = other.source_stub.clone();
        }
        if self.source_slug.is_none() {
            self.source_slug = other.source_slug.clone();
        }
        if self.source_description.is_none() {
            self.source_description = other.source_description.clone();
        }
        if self.source_date.is_none() {
            self.source_date = other.source_date.clone();
        }
        if self.source_contact.is_none() {
            self.source_contact = other.source_contact.clone();
        }
    }
    pub fn apply_file_defaults(&mut self, file: &GHubsFileConfig) {
        if self.display_group.is_none() {
            self.display_group = file.display_group.clone();
        }
        if self.display_level.is_none() {
            self.display_level = file.display_level;
        }
        if self.exclusions.is_none() {
            self.exclusions = file.exclusions.clone();
        }
        if self.organelle.is_none() {
            self.organelle = file.organelle.clone();
        }
        if self.source_name.is_none() {
            self.source_name = file.source_name.clone();
        }
        if self.source_abbreviation.is_none() {
            self.source_abbreviation = file.source_abbreviation.clone();
        }
        if self.source_url.is_none() {
            self.source_url = file.source_url.clone();
        }
        if self.source_stub.is_none() {
            self.source_stub = file.source_stub.clone();
        }
        if self.source_slug.is_none() {
            self.source_slug = file.source_slug.clone();
        }
        if self.source_description.is_none() {
            self.source_description = file.source_description.clone();
        }
        if self.source_date.is_none() {
            self.source_date = file.source_date.clone();
        }
        if self.source_contact.is_none() {
            self.source_contact = file.source_contact.clone();
        }
    }
}

impl GHubsConfig {
    pub fn apply_defaults(&mut self) {
        // 1. Fill missing file fields from defaults.file
        if let (Some(default_file), Some(file)) = (
            self.defaults.as_ref().and_then(|d| d.file.as_ref()),
            self.file.as_mut(),
        ) {
            file.merge_missing(default_file);
        }
        // 2. Fill missing attributes/identifiers from defaults
        if let Some(default_attr) = self.defaults.as_ref().and_then(|d| d.attributes.as_ref()) {
            if let Some(attrs) = self.attributes.as_mut() {
                for (_k, v) in attrs.iter_mut() {
                    v.merge_missing(default_attr);
                }
            }
        }
        if let Some(default_id) = self.defaults.as_ref().and_then(|d| d.identifiers.as_ref()) {
            if let Some(ids) = self.identifiers.as_mut() {
                for (_k, v) in ids.iter_mut() {
                    v.merge_missing(default_id);
                }
            }
        }
        // 3. Apply file keys as defaults to all attributes
        if let Some(file) = self.file.as_ref() {
            if let Some(attrs) = self.attributes.as_mut() {
                for (_attr_name, attr_cfg) in attrs.iter_mut() {
                    attr_cfg.apply_file_defaults(file);
                }
            }
        }
    }
}
// --- End default propagation helpers ---
