// use nom::bytes::complete::tag;
// use nom::sequence::delimited;

// let mut parser = tag("|");

// println!("{}", parser(line));

use std::borrow::BorrowMut;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::CString;
use std::fmt;
use std::fs::OpenOptions;
use std::hash::Hash;
use std::io::BufWriter;
use std::io::Write;
use std::os::unix::process;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow;
use blart::NodeType;
use blart::TreeMap;
use convert_case::{Case, Casing};
use cpc::{eval, units::Unit};
use csv::{ReaderBuilder, StringRecord};
use nom::{
    bytes::complete::{tag, take_until},
    combinator::map,
    multi::separated_list0,
    IResult,
};
use regex::Regex;
use schemars::JsonSchema;
use serde;
use serde::{Deserialize, Deserializer, Serialize};

use struct_iterable::Iterable;

use crate::error;
use crate::io;
use crate::taxonomy::lookup::MatchStatus;

use super::lookup::Candidate;
use super::lookup::TaxonMatch;
use super::lookup::{build_lookup, match_taxonomy_section, TaxonInfo};

/// A taxon name
#[derive(Clone, Debug, Default, Eq, Iterable, Ord, PartialEq, PartialOrd)]
pub struct Name {
    pub tax_id: String,
    pub name: String,
    pub unique_name: String,
    pub class: Option<String>,
}

impl Name {
    /// Parse a node.
    pub fn parse<'a>(input: &'a str, xref_label: &Option<String>) -> IResult<&'a str, Self> {
        // This parser outputs a Vec(&str).
        let parse_name = separated_list0(tag("\t|\t"), take_until("\t|"));
        // Map the Vec(&str) into a Node.
        map(parse_name, |v: Vec<&str>| Name {
            tax_id: v[0].to_string(),
            name: v[1].to_string(),
            unique_name: if v[2] > "" {
                v[2].to_string()
            } else if let Some(label) = &xref_label {
                format!("{}:{}", label, v[1].to_string())
            } else {
                "".to_string()
            },
            class: Some(v[3].to_string()),
            ..Default::default()
        })(input)
    }

    pub fn parse_merged(input: &str) -> IResult<&str, Self> {
        // This parser outputs a Vec(&str).
        let parse_name = separated_list0(tag("\t|\t"), take_until("\t|"));
        // Map the Vec(&str) into a Node.
        map(parse_name, |v: Vec<&str>| Name {
            tax_id: v[1].to_string(),
            name: v[0].to_string(),
            class: Some("merged taxon id".to_string()),
            ..Default::default()
        })(input)
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut values = vec![];
        for (_field_name, field_value) in self.iter() {
            if let Some(string_opt) = field_value.downcast_ref::<Option<String>>() {
                if let Some(string) = string_opt.as_deref() {
                    values.push(format!("{}", string));
                } else {
                    values.push("".to_string());
                }
            } else if let Some(string_opt) = field_value.downcast_ref::<u32>() {
                values.push(format!("{:?}", string_opt));
            } else if let Some(string_opt) = field_value.downcast_ref::<String>() {
                values.push(string_opt.clone());
            }
        }
        write!(f, "{}\t|", values.join("\t|\t"))
    }
}

/// A taxonomy node
#[derive(Clone, Debug, Default, Eq, Iterable, Ord, PartialEq, PartialOrd)]
pub struct Node {
    pub tax_id: String,
    pub parent_tax_id: String,
    pub rank: String,
    pub names: Option<Vec<Name>>,
    pub scientific_name: Option<String>,
}

const RANKS: [&str; 8] = [
    "subspecies",
    "species",
    "genus",
    "family",
    "order",
    "class",
    "phylum",
    "kingdom",
];

impl Node {
    /// Parse a node.
    pub fn parse(input: &str) -> IResult<&str, Self> {
        // This parser outputs a Vec(&str).
        let parse_node = separated_list0(tag("\t|\t"), take_until("\t|"));
        // Map the Vec(&str) into a Node.
        map(parse_node, |v: Vec<&str>| Node {
            tax_id: v[0].to_string(),
            parent_tax_id: v[1].to_string(),
            rank: v[2].to_string(),
            ..Default::default()
        })(input)
    }

    pub fn tax_id(&self) -> String {
        self.tax_id.clone()
    }

    pub fn parent_tax_id(&self) -> String {
        self.parent_tax_id.clone()
    }

    pub fn rank(&self) -> String {
        self.rank.clone()
    }

    pub fn rank_letter(&self) -> char {
        if self.rank == "subspecies" {
            return 'b';
        }
        self.rank.chars().next().unwrap()
    }

    pub fn scientific_name(&self) -> String {
        match self.scientific_name.as_ref() {
            Some(name) => name.clone(),
            None => "".to_string(),
        }
    }

    pub fn lc_tax_id(&self) -> String {
        self.tax_id.to_case(Case::Lower)
    }

    pub fn lc_scientific_name(&self) -> String {
        self.scientific_name().to_case(Case::Lower)
    }

    pub fn names_by_class(&self, classes_vec: Option<&Vec<String>>, lc: bool) -> Vec<String> {
        let mut filtered_names = vec![];
        if let Some(names) = self.names.clone() {
            for name in names {
                if let Some(classes) = classes_vec {
                    if let Some(class) = name.class {
                        if classes.contains(&class) {
                            if lc {
                                filtered_names.push(name.name.to_case(Case::Lower));
                            } else {
                                filtered_names.push(name.name.clone());
                            }
                        }
                    }
                } else if lc {
                    filtered_names.push(name.name.to_case(Case::Lower));
                } else {
                    filtered_names.push(name.name.clone());
                }
            }
        }
        filtered_names
    }

    pub fn to_taxonomy_section(&self, nodes: &Nodes) -> HashMap<String, String> {
        let mut taxonomy_section = HashMap::new();
        let root_id = "1".to_string();
        let lineage = nodes.lineage(&root_id, &self.tax_id);
        let ranks: HashSet<&str> = HashSet::from_iter(RANKS.iter().cloned());
        if ranks.contains(&self.rank as &str) {
            taxonomy_section.insert("alt_taxon_id".to_string(), self.tax_id.clone());
            taxonomy_section.insert(self.rank.clone(), self.scientific_name());
            for node in lineage {
                if ranks.contains(&node.rank as &str) {
                    taxonomy_section.insert(node.rank.clone(), node.scientific_name());
                }
            }
        }
        taxonomy_section
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let ignore = vec!["names", "scientific_name"];
        let mut values = vec![];
        for (field_name, field_value) in self.iter() {
            if !ignore.contains(&field_name) {
                //     values.push(format!("{:?}", field_value.to_string()));
                // }
                if let Some(string_opt) = field_value.downcast_ref::<Option<String>>() {
                    if let Some(string) = string_opt.as_deref() {
                        values.push(format!("{:?}", string));
                    } else {
                        values.push("".to_string());
                    }
                } else if let Some(string_opt) = field_value.downcast_ref::<u32>() {
                    values.push(format!("{:?}", string_opt));
                } else if let Some(string_opt) = field_value.downcast_ref::<String>() {
                    values.push(string_opt.clone());
                }
            }
        }
        write!(f, "{}\t|", values.join("\t|\t"))
    }
}

/// A set of taxonomy nodes
#[derive(Clone, Debug, Default, Eq, Iterable, PartialEq)]
pub struct Nodes {
    pub nodes: HashMap<String, Node>,
    pub children: HashMap<String, Vec<String>>,
}

impl Nodes {
    /// Get parent Node.
    pub fn parent(&self, taxon_id: &String) -> Option<&Node> {
        let node = self.nodes.get(taxon_id).unwrap();
        self.nodes.get(&node.parent_tax_id)
    }

    /// Get lineage from root to target.
    pub fn lineage(&self, root_id: &String, taxon_id: &String) -> Vec<&Node> {
        let mut nodes = vec![];
        let mut tax_id = taxon_id;
        if tax_id == root_id {
            return nodes;
        }
        let mut prev_tax_id = tax_id.clone();
        while tax_id != root_id {
            if let Some(node) = self.parent(&tax_id) {
                tax_id = &node.tax_id;
                nodes.push(node)
            } else {
                break;
            }
            if tax_id == &prev_tax_id {
                break;
            }
            prev_tax_id = tax_id.clone();
        }
        nodes.into_iter().rev().collect()
    }

    /// Write nodes.dmp file for a root taxon.
    pub fn write_taxdump(
        &self,
        root_ids: Vec<String>,
        base_id: Option<String>,
        nodes_writer: &mut Box<dyn Write>,
        names_writer: &mut Box<dyn Write>,
    ) -> () {
        let mut ancestors = HashSet::new();
        for root_id in root_ids {
            if let Some(lineage_root_id) = base_id.clone() {
                let lineage = self.lineage(&lineage_root_id, &root_id);
                for anc_node in lineage {
                    if !ancestors.contains(&anc_node.tax_id.clone()) {
                        writeln!(nodes_writer, "{}", &anc_node).unwrap();
                        if let Some(names) = anc_node.names.as_ref() {
                            for name in names {
                                writeln!(names_writer, "{}", &name).unwrap();
                            }
                        }
                        ancestors.insert(anc_node.tax_id.clone());
                    }
                }
            }
            if let Some(root_node) = self.nodes.get(&root_id) {
                writeln!(nodes_writer, "{}", &root_node).unwrap();
                if let Some(names) = root_node.names.as_ref() {
                    for name in names {
                        writeln!(names_writer, "{}", &name).unwrap();
                    }
                }
                if let Some(children) = self.children.get(&root_id) {
                    for child in children {
                        self.write_taxdump(vec![child.clone()], None, nodes_writer, names_writer)
                    }
                }
            }
        }
    }

    pub fn nodes_by_rank(&self, rank: &str) -> Vec<Node> {
        let mut nodes = vec![];
        for node in self.nodes.iter() {
            if node.1.rank == rank {
                nodes.push(node.1.clone());
            }
        }
        nodes
    }

    pub fn merge(&mut self, new_nodes: &Nodes) -> Result<(), anyhow::Error> {
        let nodes = &mut self.nodes;
        let children = &mut self.children;
        for node in new_nodes.nodes.iter() {
            if let Some(existing_node) = nodes.get(&node.1.tax_id) {
                if existing_node.rank == "no rank" {
                    nodes.insert(node.1.tax_id.clone(), node.1.clone());
                }
            } else {
                nodes.insert(node.1.tax_id.clone(), node.1.clone());
            }
            let parent = node.1.parent_tax_id.clone();
            let child = node.1.tax_id.clone();
            if parent != child {
                match children.entry(parent) {
                    Entry::Vacant(e) => {
                        e.insert(vec![child]);
                    }
                    Entry::Occupied(mut e) => {
                        e.get_mut().push(child);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn add_names(
        &mut self,
        new_names: &HashMap<String, Vec<Name>>,
    ) -> Result<(), anyhow::Error> {
        let nodes = &mut self.nodes;
        for (taxid, names) in new_names.iter() {
            if let Some(node) = nodes.get_mut(taxid) {
                let node_names = node.names.as_mut();
                if let Some(node_names) = node_names {
                    for name in names {
                        //check if name already exists
                        let mut found = false;
                        for node_name in node_names.iter() {
                            if node_name.name == name.name {
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            node_names.push(name.clone());
                        }
                    }
                } else {
                    node.names = Some(names.clone());
                }
            }
        }
        Ok(())
    }
}

pub fn parse_taxdump(taxdump: PathBuf, xref_label: Option<String>) -> Result<Nodes, anyhow::Error> {
    let mut nodes = HashMap::new();
    let mut children = HashMap::new();

    let mut nodes_file = taxdump.clone();

    nodes_file.push("nodes.dmp");

    // Parse nodes.dmp file
    if let Ok(lines) = io::read_lines(nodes_file) {
        for line in lines {
            if let Ok(s) = line {
                let node = Node::parse(&s).unwrap().1;
                let parent = node.parent_tax_id.clone();
                let child = node.tax_id.clone();
                if parent != child {
                    match children.entry(parent) {
                        Entry::Vacant(e) => {
                            e.insert(vec![child]);
                        }
                        Entry::Occupied(mut e) => {
                            e.get_mut().push(child);
                        }
                    }
                }

                nodes.insert(node.tax_id.clone(), node);
            }
        }
    }

    let mut names_file = taxdump.clone();
    names_file.push("names.dmp");

    // Parse names.dmp file and add to nodes
    if let Ok(lines) = io::read_lines(names_file) {
        for line in lines {
            if let Ok(s) = line {
                let name = Name::parse(&s, &xref_label).unwrap().1;
                let node = nodes.get_mut(&name.tax_id).unwrap();
                if let Some(class) = name.clone().class {
                    if class == "scientific name" {
                        node.scientific_name = Some(name.clone().name)
                    }
                }
                let mut names = node.names.as_mut();
                if let Some(names) = names.as_mut() {
                    names.push(name);
                } else {
                    node.names = Some(vec![name]);
                }
            }
        }
    }

    let mut merged_file = taxdump.clone();
    merged_file.push("merged.dmp");

    // check if merged.dmp file exists
    if !merged_file.exists() {
        return Ok(Nodes { nodes, children });
    }
    // Parse merged.dmp file and add to nodes
    if let Ok(lines) = io::read_lines(merged_file) {
        for line in lines {
            if let Ok(s) = line {
                let name = Name::parse_merged(&s).unwrap().1;
                let node = nodes.get_mut(&name.tax_id).unwrap();
                let mut names = node.names.as_mut();
                if let Some(names) = names.as_mut() {
                    names.push(name);
                } else {
                    node.names = Some(vec![name]);
                }
            }
        }
    }

    Ok(Nodes { nodes, children })
}

pub fn write_taxdump(
    nodes: &Nodes,
    root_taxon_ids: Option<Vec<String>>,
    base_taxon_id: Option<String>,
    taxdump: PathBuf,
) {
    let mut root_ids = vec![];
    match root_taxon_ids {
        Some(ids) => {
            for id in ids {
                root_ids.push(id)
            }
        }
        None => root_ids.push("1".to_string()),
    };
    let mut nodes_writer = io::get_writer(&Some(io::append_to_path(&taxdump, "/nodes.dmp")));
    let mut names_writer = io::get_writer(&Some(io::append_to_path(&taxdump, "/names.dmp")));

    nodes.write_taxdump(
        root_ids,
        base_taxon_id,
        &mut nodes_writer,
        &mut names_writer,
    );
}

pub fn parse_gbif(gbif_backbone: PathBuf) -> Result<Nodes, anyhow::Error> {
    let mut nodes = HashMap::new();
    let mut children = HashMap::new();

    nodes.insert(
        "root".to_string(),
        Node {
            tax_id: "root".to_string(),
            parent_tax_id: "root".to_string(),
            rank: "root".to_string(),
            scientific_name: None,
            names: None,
            ..Default::default()
        },
    );

    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b'\t')
        .from_path(gbif_backbone)?;

    // Status can be:
    // ACCEPTED
    // DOUBTFUL
    // HETEROTYPIC_SYNONYM
    // HOMOTYPIC_SYNONYM
    // MISAPPLIED
    // PROPARTE_SYNONYM
    // SYNONYM
    let mut ignore = HashSet::new();
    ignore.insert("DOUBTFUL");
    ignore.insert("MISAPPLIED");
    ignore.insert("HETEROTYPIC_SYNONYM");
    ignore.insert("HOMOTYPIC_SYNONYM");
    ignore.insert("PROPARTE_SYNONYM");
    ignore.insert("SYNONYM");
    for result in rdr.records() {
        let record = result?;
        let status = record.get(4).unwrap();
        if ignore.contains(status) {
            continue;
        }

        let tax_id = record.get(0).unwrap().to_string();
        let name_class = match status {
            "ACCEPTED" => "scientific name".to_string(),
            _ => "synonym".to_string(),
        };
        let taxon_name = record.get(19).unwrap().to_string();
        let mut parent_tax_id = record.get(1).unwrap().to_string();
        if parent_tax_id == "\\N" {
            parent_tax_id = "root".to_string()
        }
        let name = Name {
            tax_id: tax_id.clone(),
            name: taxon_name.clone(),
            class: Some(name_class.clone()),
            ..Default::default()
        };
        match nodes.entry(tax_id.clone()) {
            Entry::Vacant(e) => {
                let node = Node {
                    tax_id,
                    parent_tax_id,
                    rank: record.get(5).unwrap().to_case(Case::Lower),
                    scientific_name: if name_class == "scientific name" {
                        Some(taxon_name)
                    } else {
                        None
                    },
                    names: Some(vec![name]),
                    ..Default::default()
                };
                let parent = node.parent_tax_id.clone();
                let child = node.tax_id.clone();
                if parent != child {
                    match children.entry(parent) {
                        Entry::Vacant(e) => {
                            e.insert(vec![child]);
                        }
                        Entry::Occupied(mut e) => {
                            e.get_mut().push(child);
                        }
                    }
                }

                e.insert(node);
            }
            Entry::Occupied(mut e) => {
                if name_class == "scientific name" {
                    e.get_mut().scientific_name = Some(taxon_name);
                }
                if let Some(names) = e.get_mut().names.as_mut() {
                    names.push(name);
                }
            }
        }

        // println!("{:?}", record.get(0));
        // let node = Node {
        //     tax_id,
        //     parent_tax_id: record.get(1).unwrap().to_string(),
        //     rank: record.get(5).unwrap().to_case(Case::Lower),
        //     scientific_name: Some(record.get(19).unwrap().to_string()),
        //     ..Default::default()
        // };
    }
    Ok(Nodes { nodes, children })
}

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
#[derive(Default, Serialize, Deserialize, Clone, Debug, JsonSchema)]
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
    pub needs: Option<PathBufOrVec>,
    // /// File source
    // pub source: Option<Source>,
    /// Skip partial rows or cells
    /// Default: row
    /// Options: row, cell
    pub skip_partial: Option<SkipPartial>,
}

/// GenomeHubs field constraint configuration options
#[derive(Default, Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct ConstraintConfig {
    // List of valid values
    #[serde(
        rename = "enum",
        deserialize_with = "deserialize_to_lowercase",
        default
    )]
    pub enum_values: Option<Vec<String>>,
    // Value length
    pub len: Option<usize>,
    // Maximum value
    pub max: Option<f64>,
    // Minimum value
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
    pub group: Option<String>,
    // Display level
    pub level: Option<u8>,
    // Displa name
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
    pub bins: Option<BinsConfig>,
    // Constraint on field values
    pub constraint: Option<ConstraintConfig>,
    // Default value
    pub default: Option<String>,
    // Field description
    pub description: Option<String>,
    // Display options
    pub display: Option<DisplayConfig>,
    // Function to apply to value
    pub function: Option<String>,
    // Column header
    pub header: Option<StringOrVec>,
    // Column index
    pub index: Option<UsizeOrVec>,
    // String to join columns
    pub join: Option<String>,
    // Attribute key
    pub key: Option<String>,
    // Attribute name
    pub name: Option<String>,
    // Value separator
    pub separator: Option<StringOrVec>,
    // Attribute status
    pub status: Option<FieldStatus>,
    // Attribute summary functions
    pub summary: Option<StringOrVec>,
    // Attribute name synonyms
    #[serde(alias = "synonym")]
    pub synonyms: Option<StringOrVec>,
    // List of values to translate
    pub translate: Option<HashMap<String, StringOrVec>>,
    // Field type
    #[serde(rename = "type", default = "default_field_type")]
    pub field_type: FieldType,
    // Attribute value units
    #[serde(alias = "unit")]
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
#[derive(Default, Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct GHubsConfig {
    /// File configuration options
    pub file: Option<GHubsFileConfig>,
    /// Attribute fields
    pub attributes: Option<HashMap<String, GHubsFieldConfig>>,
    /// Taxon names
    pub taxon_names: Option<HashMap<String, GHubsFieldConfig>>,
    /// Taxonomy fields
    pub taxonomy: Option<HashMap<String, GHubsFieldConfig>>,
}

impl GHubsConfig {
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
        }
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
        // dbg!(&config);

        if let Some(file_config) = config.file.clone() {
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
        Some(r) => r,
        None => {
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
                ghubs_config = extra_config.merge(ghubs_config.clone());
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

fn update_config(key: &str, ghubs_config: &mut GHubsConfig, headers: &StringRecord) {
    for (_, field) in ghubs_config.borrow_mut().get_mut(key).unwrap().iter_mut() {
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
    // let mut valid = false;
    // dbg!(&field);
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
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct ValidationCounts {
    pub valid: usize,
    pub invalid: usize,
    pub partial: usize,
    pub blank: usize,
    pub errors: usize,
    pub total: usize,
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
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct ValidationReport {
    pub row_index: usize,
    pub status: ValidationStatus,
    pub counts: ValidationCounts,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub invalid: HashMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub partial: HashMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blank: Vec<String>,
    #[serde(skip)]
    pub validated: HashMap<String, String>,
    pub errors: Vec<String>,
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
    let mut valid = true;
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
    // dbg!(&field.header, &value);
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
        ValidationStatus::Invalid => false,
        ValidationStatus::Partial => false,
        ValidationStatus::Blank => true,
        ValidationStatus::Error => false,
        ValidationStatus::None => false,
    };
    if !is_valid {
        invalid_values.push(val.to_string());
    }
    ret_values.push((v, status));
}

fn validate_values(
    key: &str,
    ghubs_config: &mut GHubsConfig,
    record: &StringRecord,
) -> ValidationReport {
    let mut validated = HashMap::new();
    let mut invalid: HashMap<String, Vec<String>> = HashMap::new();
    let mut partial: HashMap<String, Vec<String>> = HashMap::new();
    let mut blank: Vec<String> = vec![];
    let mut field_counts = ValidationCounts {
        valid: 0,
        invalid: 0,
        partial: 0,
        blank: 0,
        errors: 0,
        total: 0,
    };
    let skip_partial = ghubs_config.file.as_ref().unwrap().skip_partial.clone();

    for (field_name, field) in ghubs_config.borrow_mut().get_mut(key).unwrap().iter_mut() {
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
                ValidationStatus::Invalid => false,
                ValidationStatus::Partial => false,
                ValidationStatus::Blank => true,
                ValidationStatus::Error => false,
                ValidationStatus::None => false,
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
                    field_counts.valid -= 1;
                }
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

// Add new names to the taxonomy
fn add_new_names(
    taxon: &Candidate,
    taxon_names: &HashMap<String, String>,
    names: &mut HashMap<String, Vec<Name>>,
    id_map: &TreeMap<CString, Vec<TaxonInfo>>,
) {
    if !taxon.tax_id.is_some() {
        return;
    }
    let tax_id = taxon.tax_id.clone().unwrap();
    for (name_class, name) in taxon_names.iter() {
        if name == "None" || name == "NA" {
            continue;
        }
        // does name already exist in id_map associated with the same class and taxid?
        // if so, skip for now
        if let Some(tax_info) = id_map.get(&CString::new(name.clone()).unwrap()) {
            let mut found = false;
            for info in tax_info {
                if info.tax_id == tax_id {
                    found = true;
                }
            }
            if found {
                continue;
            }
        }

        let taxon_name = Name {
            tax_id: tax_id.clone(),
            name: name.clone(),
            class: Some(name_class.clone()),
            ..Default::default()
        };

        names
            .entry(tax_id.clone())
            .or_insert(vec![])
            .push(taxon_name);
    }
}

fn add_new_taxid(
    taxon: &TaxonMatch,
    taxonomy_section: &HashMap<String, String>,
    id_map: &TreeMap<CString, Vec<TaxonInfo>>,
) -> Option<Node> {
    // check taxonomy_section has a value for alt_taxon_id that is not None or NA
    let alt_taxon_id;
    if let Some(alt_id) = taxonomy_section.get("alt_taxon_id") {
        if alt_id == "None" && alt_id == "NA" {
            return None;
        } else {
            alt_taxon_id = alt_id;
        }
    } else {
        return None;
    }
    let mut node = None;
    if let Some(higher_status) = &taxon.higher_status {
        if let MatchStatus::PutativeMatch(higher_candidate) = higher_status {
            // attach directly to higher taxon for now
            node = Some(Node {
                tax_id: alt_taxon_id.clone(),
                parent_tax_id: higher_candidate.tax_id.clone().unwrap(),
                rank: taxon.taxon.rank.clone(),
                scientific_name: Some(taxon.taxon.name.clone()),
                names: None,
                ..Default::default()
            });
        }
    }
    node
}

// Parse taxa from a GenomeHubs data file
fn nodes_from_file(
    config_file: &PathBuf,
    ghubs_config: &mut GHubsConfig,
    id_map: &TreeMap<CString, Vec<TaxonInfo>>,
    write_validated: bool,
) -> Result<(HashMap<String, Vec<Name>>, HashMap<String, Node>), error::Error> {
    let file_config = ghubs_config.file.as_ref().unwrap();
    let delimiter = match file_config.format {
        GHubsFileFormat::CSV => b',',
        GHubsFileFormat::TSV => b'\t',
    };
    let mut path = config_file.clone();
    path.pop();
    path.push(file_config.name.clone());

    let mut rdr = {
        let file = std::fs::File::open(&path).map_err(|e| {
            error::Error::FileNotFound(format!(
                "Failed to open file {}: {}\nCalled from: {}",
                path.display(),
                e,
                config_file.display()
            ))
        })?;
        let reader: Box<dyn std::io::Read> =
            if path.extension().and_then(|s| s.to_str()) == Some("gz") {
                Box::new(flate2::read::GzDecoder::new(file))
            } else {
                Box::new(file)
            };
        ReaderBuilder::new()
            .has_headers(file_config.header)
            .delimiter(delimiter)
            .from_reader(reader)
    };
    let headers = rdr.headers()?;
    let keys = vec!["attributes", "taxon_names", "taxonomy"];
    for key in keys.iter() {
        if ghubs_config.get(key).is_some() {
            update_config(key, ghubs_config, headers);
        }
    }
    let mut names = HashMap::new();
    let mut nodes = HashMap::new();
    // dbg!(&ghubs_config);

    let mut writer = if write_validated {
        let mut path = config_file.clone();
        let file_name = path.file_stem().unwrap().to_str().unwrap().to_string();
        path.pop();
        path.push("validated");
        std::fs::create_dir_all(&path).unwrap();
        path.push(file_name);
        let file = std::fs::File::create(&path).map_err(|e| {
            error::Error::FileNotFound(format!(
                "Failed to create file {}: {}\nCalled from: {}",
                path.display(),
                e,
                config_file.display()
            ))
        })?;
        Some(csv::Writer::from_writer(file))
    } else {
        None
    };

    // set up file to write exceptions as jsonl in exceptions subdirectory
    let mut exception_path = config_file.clone();
    exception_path.pop();
    exception_path.push("exceptions");
    std::fs::create_dir_all(&exception_path).unwrap();
    exception_path.push("exceptions.jsonl");
    // let exception_file = std::fs::File::create(&exception_path).unwrap();
    // let mut exception_writer = BufWriter::new(exception_file);
    if exception_path.exists() {
        std::fs::remove_file(&exception_path)?;
    }
    let mut exception_writer = OpenOptions::new()
        .append(true)
        .create(true)
        .open(exception_path)?;

    let mut counts = ValidationCounts {
        valid: 0,
        invalid: 0,
        partial: 0,
        blank: 0,
        errors: 0,
        total: 0,
    };

    // let mut encountered = HashSet::new();

    let mut ctr_assigned = 0;
    let mut ctr_unassigned = 0;

    let mut match_ctr = 0;
    let mut merge_match_ctr = 0;
    let mut mismatch_ctr = 0;
    let mut multimatch_ctr = 0;
    let mut putative_ctr = 0;
    let mut none_ctr = 0;
    let mut spellcheck_ctr = 0;
    let mut output_headers = vec![];

    for (row_index, result) in rdr.records().enumerate() {
        if let Err(err) = result {
            let report = ValidationReport {
                row_index,
                counts: ValidationCounts {
                    errors: 1,
                    total: 1,
                    ..Default::default()
                },
                status: ValidationStatus::Error,
                errors: vec![format!("Error reading record: {}", err)],
                ..Default::default()
            };
            counts.errors += 1;
            exception_writer
                .write_all(report.to_jsonl().as_bytes())
                .unwrap();
            exception_writer.write_all(b"\n").unwrap();
            continue;
        }
        let record = result?;
        let mut processed = HashMap::new();
        let mut combined_report = ValidationReport {
            row_index,
            ..Default::default()
        };
        for key in keys.iter() {
            if ghubs_config.get(key).is_some() {
                let report = validate_values(key, &mut ghubs_config.clone(), &record);
                let validated = report.validated.clone();
                combined_report.combine_reports(report);
                processed.insert(key, validated);
            }
        }

        match combined_report.status {
            ValidationStatus::Valid => counts.valid += 1,
            ValidationStatus::Invalid => counts.invalid += 1,
            ValidationStatus::Partial => counts.partial += 1,
            ValidationStatus::Blank => counts.blank += 1,
            ValidationStatus::Error => counts.errors += 1,
            ValidationStatus::None => {}
        }

        if combined_report.status != ValidationStatus::Valid {
            exception_writer
                .write_all(combined_report.to_jsonl().as_bytes())
                .unwrap();
            exception_writer.write_all(b"\n").unwrap();
            exception_writer.flush().unwrap();
            if combined_report.status == ValidationStatus::Partial {
                if ghubs_config.file.as_ref().unwrap().skip_partial == Some(SkipPartial::Row) {
                    continue;
                }
            }
        }
        // if output_headers is not set
        // flatten top-level keys in processed and store rows ready to write to CSV file
        if let Some(writer) = &mut writer {
            // flatten top-level keys in processed
            // if output_headers is not set then set it
            // write to CSV/TSV file
            if output_headers.is_empty() {
                for (key, _) in processed.iter() {
                    // output_headers.push(key.to_string());
                    for (field, _) in ghubs_config.get(key).unwrap().iter() {
                        output_headers.push((key.to_owned().clone(), field.clone()));
                    }
                }
                writer.write_record(output_headers.iter().map(|(_, field)| field))?;
            }
            let mut row = vec![];
            for (key, field) in output_headers.clone() {
                if let Some(nested) = processed.get(&key) {
                    if let Some(value) = nested.get(&field) {
                        row.push(value.clone());
                    } else {
                        row.push("None".to_string());
                    }
                }
            }
            writer.write_record(&row)?;
        }

        // dbg!(processed.clone());

        let taxonomy_section = processed.get(&"taxonomy");
        let taxon_names_section = processed.get(&"taxon_names");

        if taxonomy_section.is_none() || id_map.is_empty() {
            continue;
        }

        let (assigned_taxon, taxon_match) =
            match_taxonomy_section(taxonomy_section.unwrap(), id_map);
        if let Some(taxon) = assigned_taxon {
            ctr_assigned += 1;
            if let Some(taxon_names) = taxon_names_section {
                add_new_names(&taxon, taxon_names, &mut names, &id_map);
            }
        } else {
            ctr_unassigned += 1;
        }
        let mut unmatched = false;
        if let Some(status) = taxon_match.rank_status.as_ref() {
            match status {
                MatchStatus::Match(_) => match_ctr += 1,
                MatchStatus::MergeMatch(_) => merge_match_ctr += 1,
                MatchStatus::Mismatch(_) => mismatch_ctr += 1,
                MatchStatus::MultiMatch(_) => multimatch_ctr += 1,
                MatchStatus::PutativeMatch(_) => putative_ctr += 1,
                MatchStatus::None => {
                    none_ctr += 1;
                    unmatched = true;
                }
            }
        } else if let Some(options) = &taxon_match.rank_options {
            spellcheck_ctr += 1;
        } else {
            none_ctr += 1;
            unmatched = true;
        }
        if unmatched {
            if let Some(node) = add_new_taxid(&taxon_match, taxonomy_section.unwrap(), &id_map) {
                nodes.insert(node.tax_id.clone(), node.clone());
                if let Some(taxon_names) = taxon_names_section {
                    add_new_names(
                        &Candidate {
                            tax_id: Some(node.tax_id.clone()),
                            ..Default::default()
                        },
                        taxon_names,
                        &mut names,
                        &id_map,
                    );
                }
            }
        }
    }
    println!("{}", counts.to_json());
    println!("Assigned: {}, Unassigned: {}", ctr_assigned, ctr_unassigned);
    println!(
        "Match: {}, Merge Match: {}, Mismatch: {}, Multi Match: {}, Putative: {}, None: {}, Spellcheck: {}",
        match_ctr, merge_match_ctr, mismatch_ctr, multimatch_ctr, putative_ctr, none_ctr, spellcheck_ctr
    );
    Ok((names, nodes))
}

pub fn parse_file(
    config_file: PathBuf,
    id_map: &TreeMap<CString, Vec<TaxonInfo>>,
    write_validated: bool,
) -> Result<(Nodes, HashMap<String, Vec<Name>>, Source), error::Error> {
    // let mut children = HashMap::new();

    let mut ghubs_config = match parse_genomehubs_config(&config_file) {
        Ok(ghubs_config) => ghubs_config,
        Err(err) => return Err(err),
    };
    // let source = Source::new(&ghubs_config);
    let (names, tmp_nodes) =
        nodes_from_file(&config_file, &mut ghubs_config, &id_map, write_validated)?;
    let mut nodes = Nodes {
        nodes: HashMap::new(),
        children: HashMap::new(),
    };
    let source = Source::new(&ghubs_config);
    for (tax_id, node) in tmp_nodes.iter() {
        let mut node = node.clone();
        // TODO: set xref label as unique name
        let name = Name {
            tax_id: tax_id.clone(),
            name: node.scientific_name.clone().unwrap(),
            class: Some("scientific name".to_string()),
            ..Default::default()
        };
        if let Some(taxon_names) = names.get(tax_id) {
            let mut all_names = taxon_names.clone();
            all_names.push(name);
            node.names = Some(all_names);
        } else {
            node.names = Some(vec![name]);
        }
        let parent = node.parent_tax_id.clone();
        let child = node.tax_id();
        if parent != child {
            match nodes.children.entry(parent) {
                Entry::Vacant(e) => {
                    e.insert(vec![child]);
                }
                Entry::Occupied(mut e) => {
                    e.get_mut().push(child);
                }
            }
        }
        nodes.nodes.insert(tax_id.clone(), node);
    }

    // let mut rdr = ReaderBuilder::new()
    //     .has_headers(false)
    //     .delimiter(b'\t')
    //     .from_path(gbif_backbone)?;

    Ok((nodes, names, source))
}

/// Deserializer for lineage
fn lineage_deserialize<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let str_sequence = String::deserialize(deserializer)?;
    Ok(str_sequence
        .split(';')
        .map(|item| item.trim().to_owned())
        .collect())
}

/// ENA taxonomy record from taxonomy API
#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct EnaTaxon {
    // Unique taxon ID
    #[serde(rename = "taxId")]
    pub tax_id: String,
    // Scientific name
    #[serde(rename = "scientificName")]
    pub scientific_name: String,
    // Taxonomic rank
    pub rank: String,
    // Lineage
    #[serde(deserialize_with = "lineage_deserialize")]
    pub lineage: Vec<String>,
}

pub fn parse_ena_jsonl(
    jsonl: PathBuf,
    existing: Option<&mut Nodes>,
) -> Result<Nodes, error::Error> {
    let mut nodes = HashMap::new();
    let mut children = HashMap::new();
    let name_classes = vec!["scientific name".to_string()];
    if let Some(existing_nodes) = existing {
        let table = build_lookup(existing_nodes, &name_classes, false);

        let lines = match io::read_lines(&jsonl) {
            Ok(r) => r,
            Err(_) => {
                return Err(error::Error::FileNotFound(format!(
                    "{:?}",
                    &jsonl.as_os_str()
                )))
            }
        };

        for line in lines {
            if let Ok(json) = line {
                let taxon: EnaTaxon = serde_json::from_str(&json)?;
                let scientific_name = taxon.scientific_name;
                for names in taxon
                    .lineage
                    .into_iter()
                    .rev()
                    .collect::<Vec<String>>()
                    .windows(2)
                {
                    let key = format!(
                        "{}:{}",
                        names[0].to_case(Case::Lower),
                        names[1].to_case(Case::Lower)
                    );
                    if let Some(parent_tax_ids) = table.get(&key) {
                        if parent_tax_ids.len() == 1 {
                            let node = Node {
                                tax_id: taxon.tax_id.clone(),
                                parent_tax_id: parent_tax_ids[0].clone(),
                                rank: taxon.rank,
                                scientific_name: Some(scientific_name.clone()),
                                names: Some(vec![Name {
                                    tax_id: taxon.tax_id.clone(),
                                    name: scientific_name,
                                    class: Some("scientific name".to_string()),
                                    ..Default::default()
                                }]),
                            };
                            existing_nodes.nodes.insert(taxon.tax_id.clone(), node);
                            match existing_nodes.children.entry(parent_tax_ids[0].clone()) {
                                Entry::Vacant(e) => {
                                    e.insert(vec![taxon.tax_id]);
                                }
                                Entry::Occupied(mut e) => {
                                    e.get_mut().push(taxon.tax_id);
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    Ok(Nodes { nodes, children })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_name() {
        assert_eq!(
            Name::parse("1	|	all	|		|	synonym	|", &None).unwrap(),
            (
                "\t|",
                Name {
                    tax_id: String::from("1"),
                    name: String::from("all"),
                    class: Some(String::from("synonym")),
                    ..Default::default()
                }
            )
        );
    }

    #[test]
    fn test_parse_node() {
        assert_eq!(
            Node::parse("1	|	1	|	no rank	|").unwrap(),
            (
                "\t|",
                Node {
                    tax_id: String::from("1"),
                    parent_tax_id: String::from("1"),
                    rank: String::from("no rank"),
                    ..Default::default()
                }
            )
        );
        assert_eq!(
            Node::parse("2	|	131567	|	superkingdom	|		|	0	|	0	|	11	|	0	|	0	|	0	|	0	|	0	|		|")
                .unwrap(),
            (
                "\t|",
                Node {
                    tax_id: String::from("2"),
                    parent_tax_id: String::from("131567"),
                    rank: String::from("superkingdom"),
                    ..Default::default()
                }
            )
        );
    }
}
