use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::str::FromStr;

use cpc::{eval, units::Unit};
use csv::StringRecord;

use regex::Regex;
use schemars::JsonSchema;
use serde;
use serde::{Deserialize, Deserializer, Serialize};

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
pub struct GHubsFileConfig {
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
    // /// File source
    // pub source: Option<Source>,
    /// Skip partial rows or cells
    /// Default: row
    /// Options: row, cell
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_partial: Option<SkipPartial>,
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

/// GenomeHubs field constraint configuration options
#[derive(Default, Serialize, Deserialize, Clone, Debug, JsonSchema)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub len: Option<usize>,
    // Maximum value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    // Minimum value
    #[serde(skip_serializing_if = "Option::is_none")]
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

/// GenomeHubs value bins configuration options
#[derive(Default, Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct BinsConfig {
    // List of valid values
    pub count: u32,
    // Geographic resolution (hexagonal)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h3res: Option<u8>,
    // Maximum value
    pub max: f64,
    // Minimum value
    pub min: f64,
    // Value length
    pub scale: FieldScale,
}

/// GenomeHubs field display configuration options
#[derive(Default, Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct DisplayConfig {
    // Display group
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    // Display level
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    // Display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// GenomeHubs field status values
#[derive(Default, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum FieldStatus {
    // Temporary
    #[default]
    #[serde(rename = "temporary")]
    Temporary,
}

/// GenomeHubs field configuration options
#[derive(Default, Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct GHubsFieldConfig {
    // Default settings for value bins
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bins: Option<BinsConfig>,
    // Constraint on field values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraint: Option<ConstraintConfig>,
    // Default value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    // Field description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    // Display options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<DisplayConfig>,
    // Function to apply to value
    #[serde(skip_serializing)]
    pub function: Option<String>,
    // Column header
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<StringOrVec>,
    // Column index
    #[serde(skip_serializing)]
    pub index: Option<UsizeOrVec>,
    // String to join columns
    #[serde(skip_serializing)]
    pub join: Option<String>,
    // Attribute key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    // Attribute name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    // Value separator
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<StringOrVec>,
    // Attribute status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<FieldStatus>,
    // Attribute summary functions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<StringOrVec>,
    // Attribute name synonyms
    #[serde(alias = "synonym", skip_serializing_if = "Option::is_none")]
    pub synonyms: Option<StringOrVec>,
    // List of values to translate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translate: Option<HashMap<String, StringOrVec>>,
    // Field type
    #[serde(rename = "type", default = "default_field_type")]
    pub field_type: FieldType,
    // Attribute value units
    #[serde(alias = "unit", skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
}

fn default_field_type() -> FieldType {
    FieldType::Keyword
}

impl GHubsFieldConfig {
    fn merge(self, other: GHubsFieldConfig) -> Self {
        Self {
            bins: self.bins.or(other.bins),
            constraint: self.constraint.or(other.constraint),
            default: self.default.or(other.default),
            description: self.description.or(other.description),
            display: self.display.or(other.display),
            function: self.function.or(other.function),
            header: self.header.or(other.header),
            index: self.index.or(other.index),
            join: self.join.or(other.join),
            key: self.key.or(other.key),
            name: self.name.or(other.name),
            separator: self.separator.or(other.separator),
            status: self.status.or(other.status),
            summary: self.summary.or(other.summary),
            synonyms: self.synonyms.or(other.synonyms),
            translate: self.translate.or(other.translate),
            field_type: self.field_type,
            units: self.units.or(other.units),
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
pub struct GHubsConfig {
    /// File configuration options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<GHubsFileConfig>,
    /// Attribute fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<HashMap<String, GHubsFieldConfig>>,
    /// Taxon names
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_names: Option<HashMap<String, GHubsFieldConfig>>,
    /// Taxonomy fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxonomy: Option<HashMap<String, GHubsFieldConfig>>,
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

    pub fn init_csv_reader(&mut self, keys: Option<Vec<&str>>) -> csv::Reader<Box<dyn BufRead>> {
        let file_config = self.file.clone().unwrap();
        let config_path = self.file_path.clone();
        let file_path = file_config.file_path(&config_path, None);
        let delimiter = match file_config.format {
            GHubsFileFormat::CSV => b',',
            GHubsFileFormat::TSV => b'\t',
        };
        if !file_path.exists() {
            panic!("File does not exist: {:?}", &file_path);
        }
        let mut rdr = io::get_csv_reader(&Some(file_path.clone()), delimiter, file_config.header);

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
        rdr
    }

    pub fn init_file_writers(&mut self, write_validated: bool, write_exceptions: bool) -> () {
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
        let mut rdr = io::get_csv_reader(&Some(file_path), delimiter, true);
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
#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct Source {
    /// Source name
    #[serde(rename = "source")]
    pub name: String,
    /// Source abbreviation
    pub abbreviation: Option<String>,
    /// Source URL (Single URL for all values)
    #[serde(rename = "source_url")]
    pub url: Option<String>,
    /// Source URL stub (base URL for values)
    #[serde(rename = "source_url_stub")]
    pub stub: Option<String>,
    /// Source URL suffix (suffix for values)
    #[serde(rename = "source_slug")]
    pub slug: Option<String>,
    /// Source description
    #[serde(rename = "source_description")]
    pub description: Option<String>,
    /// Source last updated date
    #[serde(rename = "source_date")]
    pub date: Option<String>,
    /// Source contact name
    #[serde(rename = "source_contact")]
    pub contact: Option<String>,
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

fn translate_value(field: &GHubsFieldConfig, value: &String) -> Vec<String> {
    let mut values = vec![];
    if let Some(ref translate) = field.translate {
        let translated = translate
            .get(value)
            .cloned()
            .unwrap_or(StringOrVec::Single(value.to_owned()));
        match translated {
            StringOrVec::Single(val) => values.push(val),
            StringOrVec::Multiple(vals) => values.extend(vals),
        };
    } else {
        values.push(value.to_owned());
    }
    values
}

fn process_value(
    value: String,
    field: &GHubsFieldConfig,
) -> Result<
    (
        Vec<(String, ValidationStatus)>,
        Vec<String>,
        ValidationStatus,
    ),
    error::Error,
> {
    let values = translate_value(field, &value);
    let mut ret_values = vec![];
    let mut invalid_values = vec![];
    for value in values {
        if let Some(separator) = &field.separator {
            let re = match separator {
                StringOrVec::Single(sep) => Regex::new(sep).unwrap(),
                StringOrVec::Multiple(separators) => Regex::new(
                    separators
                        // .iter()
                        // .map(|sep| record.get(idx.to_owned()).unwrap_or(""))
                        // .collect::<Vec<&str>>()
                        .join(&"|")
                        .as_str(),
                )
                .unwrap(),
            };
            for val in re.split(value.as_str()) {
                validate_value(field, &mut ret_values, &mut invalid_values, val.to_string());
            }
        } else {
            validate_value(field, &mut ret_values, &mut invalid_values, value.clone());
        }
    }
    let status = if invalid_values.is_empty() {
        ValidationStatus::Valid
    } else if invalid_values.len() < ret_values.len() {
        ValidationStatus::Partial
    } else {
        ValidationStatus::Invalid
    };
    Ok((ret_values, invalid_values, status))
}

fn validate_value(
    field: &GHubsFieldConfig,
    ret_values: &mut Vec<(String, ValidationStatus)>,
    invalid_values: &mut Vec<String>,
    val: String,
) {
    let (v, status) = apply_function(val.to_string(), &field);
    let is_valid = match status {
        ValidationStatus::Valid => true,
        ValidationStatus::Blank => true,
        _ => false,
    };
    if !is_valid {
        invalid_values.push(val.to_string());
    }
    ret_values.push((v, status));
}
