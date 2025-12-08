use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::BufRead;
use std::path::PathBuf;

use anyhow;
use convert_case::{Case, Casing};
use nom::{
    bytes::complete::{tag, take_until},
    combinator::map,
    multi::separated_list0,
    IResult,
};
use serde::Serialize;
use serde_json::Value;
use struct_iterable::Iterable;

use crate::cli::{TaxonomyFormat, TaxonomyOptions};
use crate::io;
use crate::io::file_reader;
use crate::parse::lookup::build_fast_lookup;
use crate::parse::parse_file;

/// A taxon name
#[derive(Clone, Debug, Default, Eq, Iterable, Ord, PartialEq, PartialOrd, Serialize)]
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
#[derive(Clone, Debug, Eq, Iterable, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Node {
    pub tax_id: String,
    pub parent_tax_id: String,
    pub rank: String,
    pub columns: Vec<String>,
    pub names: Option<Vec<Name>>,
    pub scientific_name: Option<String>,
    pub row_index: Option<usize>,
    pub raw_row: Option<String>,
}

impl Default for Node {
    fn default() -> Self {
        Node {
            tax_id: String::new(),
            parent_tax_id: String::new(),
            rank: String::new(),
            columns: vec!["".to_string(); 13],
            names: None,
            scientific_name: None,
            row_index: None,
            raw_row: None,
        }
    }
}

impl Node {
    pub fn to_json(&self, nodes: &Nodes) -> Value {
        let lineage = nodes.lineage(&"1".to_string(), &self.tax_id);
        // return
        // parent, taxon_rank, taxon_names, taxon_id, scientific_name and lineage as json
        #[derive(Serialize)]
        struct NodeJson {
            parent: String,
            taxon_rank: String,
            taxon_names: Option<Vec<TaxonName>>,
            taxon_id: String,
            scientific_name: Option<String>,
            lineage: Vec<LineageNode>,
        }
        #[derive(Serialize)]
        struct LineageNode {
            taxon_id: String,
            scientific_name: Option<String>,
            taxon_rank: String,
            node_depth: u16,
        }
        let mut lineage_json: Vec<LineageNode> = Vec::new();
        lineage_json.push(LineageNode {
            taxon_id: self.tax_id.clone(),
            scientific_name: self.scientific_name.clone(),
            taxon_rank: self.rank.clone(),
            node_depth: 0u16,
        });
        for (i, n) in lineage.iter().rev().enumerate() {
            lineage_json.push(LineageNode {
                taxon_id: n.tax_id.clone(),
                scientific_name: n.scientific_name.clone(),
                taxon_rank: n.rank.clone(),
                node_depth: i as u16 + 1u16,
            });
        }

        let name_classes: Vec<String> = vec![
            "scientific name".to_string(),
            "common name".to_string(),
            "synonym".to_string(),
            "genbank common name".to_string(),
            "merged taxon id".to_string(),
            "tolid prefix".to_string(),
            "xref".to_string(),
        ];

        #[derive(Default)]
        struct NameSource {
            pub source: Option<String>,
            pub source_url: Option<String>,
            pub source_url_stub: Option<String>,
        }

        let name_sources = HashMap::from([
            (
                "ncbi",
                NameSource {
                    source: Some("NCBI Taxonomy".to_string()),
                    source_url: Some("https://www.ncbi.nlm.nih.gov/datasets/taxonomy".to_string()),
                    source_url_stub: Some(
                        "https://www.ncbi.nlm.nih.gov/datasets/taxonomy/".to_string(),
                    ),
                    ..Default::default()
                },
            ),
            (
                "gbif",
                NameSource {
                    source: Some("GBIF Backbone Taxonomy".to_string()),
                    source_url: Some("https://www.gbif.org/".to_string()),
                    source_url_stub: Some("https://www.gbif.org/species/".to_string()),
                    ..Default::default()
                },
            ),
            (
                "ott",
                NameSource {
                    source: Some("Open Tree of Life".to_string()),
                    source_url: Some("https://tree.opentreeoflife.org/about/taxonomy".to_string()),
                    source_url_stub: Some(
                        "https://tree.opentreeoflife.org/taxonomy/browse?id=".to_string(),
                    ),
                    ..Default::default()
                },
            ),
            (
                "tolid",
                NameSource {
                    source: Some("Tree of Life ID".to_string()),
                    source_url: Some("https://id.tol.sanger.ac.uk".to_string()),
                    ..Default::default()
                },
            ),
            (
                "ena",
                NameSource {
                    source: Some("ENA Taxonomy".to_string()),
                    source_url: Some("https://www.ebi.ac.uk/ena/browser/home".to_string()),
                    source_url_stub: Some(
                        "https://www.ebi.ac.uk/ena/browser/view/Taxon:".to_string(),
                    ),
                    ..Default::default()
                },
            ),
            (
                "worms",
                NameSource {
                    source: Some("WoRMS".to_string()),
                    source_url: Some("https://www.marinespecies.org/".to_string()),
                    source_url_stub: Some(
                        "https://www.marinespecies.org/aphia.php?p=taxdetails&id=".to_string(),
                    ),
                    ..Default::default()
                },
            ),
            (
                "silva",
                NameSource {
                    source: Some("SILVA".to_string()),
                    source_url: Some("https://www.arb-silva.de/".to_string()),
                    ..Default::default()
                },
            ),
            (
                "irmng",
                NameSource {
                    source: Some("IRMNG".to_string()),
                    source_url: Some("https://www.irmng.org/".to_string()),
                    source_url_stub: Some(
                        "https://www.irmng.org/aphia.php?p=taxdetails&id=".to_string(),
                    ),
                    ..Default::default()
                },
            ),
            (
                "fung",
                NameSource {
                    source: Some("Fungidb".to_string()),
                    source_url: Some("https://fungidb.org/".to_string()),
                    ..Default::default()
                },
            ),
        ]);

        let taxon_names = self.full_names_by_class(Some(&name_classes));

        #[derive(Serialize)]
        struct TaxonName {
            name: String,
            class: Option<String>,
            source: Option<String>,
            source_url: Option<String>,
            source_url_stub: Option<String>,
        }

        let taxon_names: Option<Vec<TaxonName>> = if !taxon_names.is_empty() {
            let mut names_out: Vec<TaxonName> = vec![];
            for name in taxon_names {
                if name.unique_name == "" {
                    names_out.push(TaxonName {
                        name: name.name,
                        class: name.class,
                        source: None,
                        source_url: None,
                        source_url_stub: None,
                    });
                    continue;
                }
                // split on : to find source
                let parts: Vec<&str> = name.unique_name.splitn(2, ':').collect();
                let (source, source_url, source_url_stub) = if parts.len() == 2 {
                    if let Some(ns) = name_sources.get(parts[0]) {
                        (
                            ns.source.clone(),
                            ns.source_url.clone(),
                            ns.source_url_stub.clone(),
                        )
                    } else {
                        (None, None, None)
                    }
                } else {
                    (None, None, None)
                };
                names_out.push(TaxonName {
                    name: name.name,
                    class: name.class,
                    source,
                    source_url,
                    source_url_stub,
                });
            }
            Some(names_out)
        } else {
            None
        };

        let node_json = NodeJson {
            parent: self.parent_tax_id.clone(),
            taxon_rank: self.rank.clone(),
            taxon_names,
            taxon_id: self.tax_id.clone(),
            scientific_name: self.scientific_name.clone(),
            lineage: lineage_json,
        };
        serde_json::to_value(node_json).unwrap_or(Value::Null)
    }
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
        map(parse_node, |v: Vec<&str>| {
            let mut columns: Vec<String> = v.iter().map(|s| s.to_string()).collect();
            columns.resize(13, "".to_string());
            Node {
                tax_id: columns[0].clone(),
                parent_tax_id: columns[1].clone(),
                rank: columns[2].clone(),
                columns,
                ..Default::default()
            }
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
        self.rank.chars().next().unwrap_or(' ')
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

    pub fn full_names_by_class(&self, classes_vec: Option<&Vec<String>>) -> Vec<Name> {
        let mut filtered_names = vec![];
        if let Some(names) = self.names.clone() {
            for name in names {
                if let Some(classes) = classes_vec {
                    if let Some(ref class) = name.class {
                        if classes.contains(class) {
                            filtered_names.push(name.clone());
                        }
                    }
                } else {
                    filtered_names.push(name.clone());
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
        let mut cols = self.columns.clone();
        if cols.len() < 13 {
            cols.resize(13, "".to_string());
        }
        if !cols.is_empty() {
            cols[0] = self.tax_id.clone();
        }
        if cols.len() > 1 {
            cols[1] = self.parent_tax_id.clone();
        }
        if cols.len() > 2 {
            cols[2] = self.rank.clone();
        }
        write!(f, "{}\t|", cols.join("\t|\t"))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MergeException {
    pub tax_id: String,
    pub row_index: Option<usize>,
    pub raw_row: Option<String>,
    pub reason: String,
}

/// A set of taxonomy nodes
#[derive(Clone, Debug, Default, Eq, Iterable, PartialEq)]
pub struct Nodes {
    pub nodes: HashMap<String, Node>,
    pub children: HashMap<String, Vec<String>>,
}

impl Nodes {
    /// Create a new Nodes struct.
    pub fn new() -> Nodes {
        Nodes {
            nodes: HashMap::new(),
            children: HashMap::new(),
        }
    }

    /// Get parent Node.
    pub fn parent(&self, taxon_id: &String) -> Option<&Node> {
        let node = match self.nodes.get(taxon_id) {
            Some(n) => n,
            None => return None,
        };
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
        root_taxon_ids: Option<Vec<String>>,
        leaf_taxon_ids: Option<HashSet<String>>,
        base_id: Option<String>,
        taxdump_path: &PathBuf,
        format: Vec<TaxonomyFormat>,
        append: bool,
    ) -> () {
        use std::collections::HashSet;
        let nodes_path = io::append_to_path(taxdump_path, "/nodes.dmp");
        let names_path = io::append_to_path(taxdump_path, "/names.dmp");
        let jsonl_path = io::append_to_path(taxdump_path, "/nodes.jsonl");

        let mut jsonl_writer = if append {
            io::get_append_writer(&Some(jsonl_path.clone()))
        } else {
            io::get_writer(&Some(jsonl_path.clone()))
        };

        let mut nodes_writer = if append {
            io::get_append_writer(&Some(nodes_path.clone()))
        } else {
            io::get_writer(&Some(nodes_path.clone()))
        };
        let mut names_writer = if append {
            io::get_append_writer(&Some(names_path.clone()))
        } else {
            io::get_writer(&Some(names_path.clone()))
        };

        let mut visited = HashSet::new();
        self.write_taxdump_inner(
            root_taxon_ids,
            leaf_taxon_ids,
            base_id,
            taxdump_path,
            append,
            &format,
            &mut nodes_writer,
            &mut names_writer,
            &mut jsonl_writer,
            &mut visited,
        );
    }

    fn write_taxdump_inner(
        &self,
        root_taxon_ids: Option<Vec<String>>,
        leaf_taxon_ids: Option<HashSet<String>>,
        base_id: Option<String>,
        taxdump_path: &PathBuf,
        _append: bool,
        format: &Vec<TaxonomyFormat>,
        nodes_writer: &mut dyn std::io::Write,
        names_writer: &mut dyn std::io::Write,
        jsonl_writer: &mut dyn std::io::Write,
        visited: &mut HashSet<String>,
    ) {
        // Find all root nodes if not specified
        let mut root_ids = vec![];
        match root_taxon_ids {
            Some(ids) => {
                for id in ids {
                    root_ids.push(id)
                }
            }
            None => {
                let all_tax_ids: std::collections::HashSet<_> =
                    self.nodes.keys().cloned().collect();
                for (tax_id, node) in self.nodes.iter() {
                    if node.parent_tax_id.is_empty()
                        || !all_tax_ids.contains(&node.parent_tax_id)
                        || node.parent_tax_id == *tax_id
                    {
                        root_ids.push(tax_id.clone());
                    }
                }
            }
        };

        let mut ancestors = HashSet::new();
        for root_id in root_ids {
            visited.insert(root_id.clone());
            if let Some(lineage_root_id) = base_id.clone() {
                let lineage = self.lineage(&lineage_root_id, &root_id);
                for anc_node in lineage {
                    if !ancestors.contains(&anc_node.tax_id.clone()) {
                        if format.contains(&TaxonomyFormat::JSONL) {
                            writeln!(jsonl_writer, "{}", anc_node.to_json(self)).unwrap();
                            jsonl_writer.flush().unwrap();
                        }
                        if format.contains(&TaxonomyFormat::NCBI) {
                            writeln!(nodes_writer, "{}", &anc_node).unwrap();
                            nodes_writer.flush().unwrap();
                            if let Some(names) = anc_node.names.as_ref() {
                                for name in names {
                                    writeln!(names_writer, "{}", &name).unwrap();
                                    names_writer.flush().unwrap();
                                }
                            }
                        }
                        ancestors.insert(anc_node.tax_id.clone());
                    }
                }
            }
            if let Some(root_node) = self.nodes.get(&root_id) {
                if format.contains(&TaxonomyFormat::JSONL) {
                    writeln!(jsonl_writer, "{}", root_node.to_json(self)).unwrap();
                    jsonl_writer.flush().unwrap();
                }
                if format.contains(&TaxonomyFormat::NCBI) {
                    writeln!(nodes_writer, "{}", &root_node).unwrap();
                    nodes_writer.flush().unwrap();
                    if let Some(names) = root_node.names.as_ref() {
                        for name in names {
                            writeln!(names_writer, "{}", &name).unwrap();
                            names_writer.flush().unwrap();
                        }
                    }
                }
                let mut is_leaf = false;
                if let Some(ref leaf_ids) = leaf_taxon_ids {
                    if leaf_ids.contains(&root_id) {
                        is_leaf = true;
                    }
                }
                if let Some(children) = self.children.get(&root_id) {
                    for child in children {
                        if is_leaf {
                            if let Some(ref leaf_ids) = leaf_taxon_ids {
                                if !leaf_ids.contains(child) {
                                    continue;
                                }
                            }
                        }
                        self.write_taxdump_inner(
                            Some(vec![child.clone()]),
                            leaf_taxon_ids.clone(),
                            None,
                            taxdump_path,
                            true,
                            &format,
                            nodes_writer,
                            names_writer,
                            jsonl_writer,
                            visited,
                        );
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

    pub fn merge(&mut self, new_nodes: &Nodes) -> Result<Vec<MergeException>, anyhow::Error> {
        let nodes = &mut self.nodes;
        let children = &mut self.children;
        let mut exceptions = Vec::new();
        for node in new_nodes.nodes.values() {
            // Prevent self-parenting
            if node.tax_id == node.parent_tax_id {
                exceptions.push(MergeException {
                    tax_id: node.tax_id.clone(),
                    row_index: node.row_index,
                    raw_row: node.raw_row.clone(),
                    reason: "Self-parenting detected".to_string(),
                });
                continue;
            }
            // Prevent cycles: check if parent is a descendant of this node
            let mut ancestor = node.parent_tax_id.clone();
            let mut cycle = false;
            while let Some(parent_node) = nodes.get(&ancestor) {
                if parent_node.tax_id == node.tax_id {
                    cycle = true;
                    break;
                }
                if parent_node.tax_id == parent_node.parent_tax_id {
                    break;
                }
                ancestor = parent_node.parent_tax_id.clone();
            }
            if cycle {
                exceptions.push(MergeException {
                    tax_id: node.tax_id.clone(),
                    row_index: node.row_index,
                    raw_row: node.raw_row.clone(),
                    reason: "Cycle detected".to_string(),
                });
                continue;
            }
            // Always insert/replace the node
            nodes.insert(node.tax_id.clone(), node.clone());
            let parent = node.parent_tax_id.clone();
            let child = node.tax_id.clone();
            if parent != child {
                match children.entry(parent) {
                    Entry::Vacant(e) => {
                        e.insert(vec![child]);
                    }
                    Entry::Occupied(mut e) => {
                        if !e.get().contains(&child) {
                            e.get_mut().push(child);
                        }
                    }
                }
            }
        }
        Ok(exceptions)
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

    /// Parse a taxdump directory into a Nodes struct.
    pub fn from_taxdump(
        taxdump: PathBuf,
        xref_label: Option<String>,
    ) -> Result<Nodes, crate::error::Error> {
        let mut nodes = HashMap::new();
        let mut children = HashMap::new();

        let mut nodes_file = taxdump.clone();

        nodes_file.push("nodes.dmp");

        // Parse nodes.dmp file
        if let Ok(lines) = io::read_lines(nodes_file) {
            for (row_index, line) in lines.enumerate() {
                if let Ok(s) = line {
                    let mut node = match Node::parse(&s) {
                        Ok((_, n)) => n,
                        Err(_) => continue,
                    };
                    node.row_index = Some(row_index);
                    node.raw_row = Some(s.clone());
                    let parent = node.parent_tax_id.clone();
                    let child = node.tax_id.clone();
                    if parent != child {
                        match children.entry(parent) {
                            Entry::Vacant(e) => {
                                e.insert(vec![child.clone()]);
                            }
                            Entry::Occupied(mut e) => {
                                e.get_mut().push(child.clone());
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
                    let name = match Name::parse(&s, &xref_label) {
                        Ok((_, n)) => n,
                        Err(_) => continue,
                    };
                    if let Some(node) = nodes.get_mut(&name.tax_id) {
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
                    let name = match Name::parse_merged(&s) {
                        Ok((_, n)) => n,
                        Err(_) => continue,
                    };
                    if let Some(node) = nodes.get_mut(&name.tax_id) {
                        let mut names = node.names.as_mut();
                        if let Some(names) = names.as_mut() {
                            names.push(name);
                        } else {
                            node.names = Some(vec![name]);
                        }
                    }
                }
            }
        }

        Ok(Nodes { nodes, children })
    }

    pub fn from_ott(
        ott_path: PathBuf,
        _options: &TaxonomyOptions,
        existing: Option<&mut Nodes>,
    ) -> Result<Nodes, crate::error::Error> {
        use std::collections::hash_map::Entry;
        use std::fs::File;
        use std::io::{BufRead, BufReader};
        let (mut nodes, mut children) = if let Some(existing_nodes) = existing {
            (
                existing_nodes.nodes.clone(),
                existing_nodes.children.clone(),
            )
        } else {
            (HashMap::new(), HashMap::new())
        };
        // Map xref label -> tax_id for synonym lookup
        let mut xref_to_taxid: HashMap<String, String> = HashMap::new();
        // Parse taxonomy.tsv with correct OTT separator (\t|\t)
        let mut taxonomy_file = ott_path.clone();
        taxonomy_file.push("taxonomy.tsv");
        let file = File::open(&taxonomy_file).map_err(crate::error::Error::from)?;
        let reader = BufReader::new(file);
        for (row_index, line) in reader.lines().enumerate() {
            let line = line.map_err(crate::error::Error::from)?;

            if line.starts_with("uid\t") {
                continue;
            }
            let fields: Vec<&str> = line.split("\t|\t").collect();
            if fields.len() < 5 {
                continue;
            }
            let tax_id = fields[0].trim().to_string();
            let parent_tax_id = if fields[1].trim().is_empty() {
                "root".to_string()
            } else {
                fields[1].trim().to_string()
            };
            let name = fields[2].trim().to_string();
            let rank = fields[3].trim().to_string();
            let xrefs = fields[4];
            // Build names: main scientific name
            let mut names = vec![Name {
                tax_id: tax_id.clone(),
                name: name.clone(),
                unique_name: format!("ott:{}", tax_id),
                class: Some("scientific name".to_string()),
                ..Default::default()
            }];
            // Add xrefs as Name with class xref, and build xref->taxid map
            for xref in xrefs.split(',') {
                let xref = xref.trim();
                if !xref.is_empty() {
                    xref_to_taxid.insert(xref.to_string(), tax_id.clone());
                    names.push(Name {
                        tax_id: tax_id.clone(),
                        name: xref.to_string(),
                        unique_name: xref.to_string(),
                        class: Some("xref".to_string()),
                        ..Default::default()
                    });
                }
            }
            let node = Node {
                tax_id: tax_id.clone(),
                parent_tax_id: parent_tax_id.clone(),
                rank: rank.to_case(Case::Lower),
                scientific_name: Some(name.clone()),
                names: Some(names),
                row_index: Some(row_index),
                raw_row: Some(line.clone()),
                ..Default::default()
            };
            let parent = node.parent_tax_id.clone();
            let child = node.tax_id.clone();
            if parent != child {
                match children.entry(parent) {
                    Entry::Vacant(e) => {
                        e.insert(vec![child.clone()]);
                    }
                    Entry::Occupied(mut e) => {
                        e.get_mut().push(child.clone());
                    }
                }
            }
            nodes.insert(child, node);
        }
        // Parse synonyms.tsv and add as synonym names to the correct node
        let mut synonyms_file = ott_path.clone();
        synonyms_file.push("synonyms.tsv");
        if synonyms_file.exists() {
            let file = File::open(&synonyms_file).map_err(crate::error::Error::from)?;
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let line = line.map_err(crate::error::Error::from)?;
                if line.starts_with("synonym\t") {
                    continue;
                }
                let fields: Vec<&str> = line.split("\t|\t").collect();
                if fields.len() < 2 {
                    continue;
                }
                let synonym = fields[0].trim();
                let tax_id = fields[1].trim();
                let sourceinfo = if fields.len() > 4 {
                    fields[4].trim()
                } else {
                    ""
                };
                // Build unique_name as <prefix>:<synonym> if sourceinfo contains a prefix
                let unique_name = if let Some((prefix, _)) = sourceinfo.split_once(':') {
                    format!("{}:{}", prefix, synonym)
                } else {
                    synonym.to_string()
                };
                if let Some(node) = nodes.get_mut(tax_id) {
                    let mut found = false;
                    if let Some(ref mut node_names) = node.names {
                        for n in node_names.iter() {
                            if n.name == synonym && n.class.as_deref() == Some("synonym") {
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            node_names.push(Name {
                                tax_id: tax_id.to_string(),
                                name: synonym.to_string(),
                                unique_name,
                                class: Some("synonym".to_string()),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
        // ...existing forwards.tsv logic unchanged...
        let mut forwards_file = ott_path.clone();
        forwards_file.push("forwards.tsv");
        if forwards_file.exists() {
            let file = File::open(&forwards_file).map_err(crate::error::Error::from)?;
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let line = line.map_err(crate::error::Error::from)?;
                if line.starts_with("id\t") {
                    continue;
                }
                let fields: Vec<&str> = line.split('\t').collect();
                if fields.len() < 2 {
                    continue;
                }
                let merged_id = fields[0];
                let replacement_id = fields[1];
                if let Some(node) = nodes.get_mut(replacement_id) {
                    let mut found = false;
                    if let Some(ref mut node_names) = node.names {
                        for n in node_names.iter() {
                            if n.name == merged_id {
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            node_names.push(Name {
                                tax_id: replacement_id.to_string(),
                                name: merged_id.to_string(),
                                unique_name: merged_id.to_string(),
                                class: Some("merged taxon id".to_string()),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
        Ok(Nodes { nodes, children })
    }

    pub fn from_gbif(
        gbif_backbone: PathBuf,
        options: &TaxonomyOptions,
        existing: Option<&mut Nodes>,
    ) -> Result<Nodes, crate::error::Error> {
        let mut nodes;
        let mut children;
        if let Some(existing_nodes) = existing {
            nodes = existing_nodes.nodes.clone();
            children = existing_nodes.children.clone();
        } else {
            nodes = HashMap::new();
            children = HashMap::new();
        }
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
        let mut rdr = io::get_csv_reader(&Some(gbif_backbone), b'\t', false, None, 0, false);
        let mut ignore = HashSet::new();
        ignore.insert("DOUBTFUL");
        ignore.insert("MISAPPLIED");
        ignore.insert("HETEROTYPIC_SYNONYM");
        ignore.insert("HOMOTYPIC_SYNONYM");
        ignore.insert("PROPARTE_SYNONYM");
        ignore.insert("SYNONYM");
        for (row_index, result) in rdr.records().enumerate() {
            let record = result.map_err(crate::error::Error::from)?;
            let status = match record.get(4) {
                Some(s) => s,
                None => continue,
            };
            if ignore.contains(status) {
                continue;
            }
            let tax_id = match record.get(0) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let name_class = match status {
                "ACCEPTED" => "scientific name".to_string(),
                _ => "synonym".to_string(),
            };
            let taxon_name = match record.get(19) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let mut parent_tax_id = match record.get(1) {
                Some(s) => s.to_string(),
                None => "root".to_string(),
            };
            if parent_tax_id == "\\N" {
                parent_tax_id = "root".to_string()
            }
            let unique_name = if let Some(xref_label) = options.xref_label.clone() {
                format!("{}:{}", xref_label, tax_id)
            } else {
                "".to_string()
            };
            let name = Name {
                tax_id: tax_id.clone(),
                name: taxon_name.clone(),
                unique_name,
                class: Some(name_class.clone()),
                ..Default::default()
            };
            match nodes.entry(tax_id.clone()) {
                Entry::Vacant(e) => {
                    let node = Node {
                        tax_id,
                        parent_tax_id,
                        rank: match record.get(5) {
                            Some(r) => r.to_case(Case::Lower),
                            None => "".to_string(),
                        },
                        scientific_name: if name_class == "scientific name" {
                            Some(taxon_name)
                        } else {
                            None
                        },
                        names: Some(vec![name]),
                        row_index: Some(row_index),
                        raw_row: Some(record.iter().collect::<Vec<_>>().join("\t")),
                        ..Default::default()
                    };
                    let parent = node.parent_tax_id.clone();
                    let child = node.tax_id.clone();
                    if parent != child {
                        match children.entry(parent) {
                            Entry::Vacant(e) => {
                                e.insert(vec![child.clone()]);
                            }
                            Entry::Occupied(mut e) => {
                                e.get_mut().push(child.clone());
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
        }
        Ok(Nodes { nodes, children })
    }

    pub fn from_jsonl(
        jsonl_path: PathBuf,
        options: &TaxonomyOptions,
        existing: Option<&mut Nodes>,
    ) -> Result<Nodes, crate::error::Error> {
        let xref_label = options
            .xref_label
            .clone()
            .unwrap_or_else(|| "ena".to_string());
        let name_classes = vec!["scientific name".to_string()];
        let nodes = HashMap::new();
        let children = HashMap::new();
        if let Some(existing_nodes) = existing {
            let table = crate::parse::lookup::build_lookup(existing_nodes, &name_classes, false);
            let reader = file_reader(jsonl_path).map_err(crate::error::Error::from)?;
            for (row_index, line) in reader.lines().enumerate() {
                let line = line.map_err(crate::error::Error::from)?;
                let v: Value = serde_json::from_str(&line).map_err(crate::error::Error::from)?;
                let tax_id = v["taxId"].as_str().unwrap_or("").to_string();
                let rank = v["rank"].as_str().unwrap_or("").to_string();
                let scientific_name = v["scientificName"].as_str().unwrap_or("").to_string();
                // Parse lineage as Vec<String>
                let lineage: Vec<String> = if let Some(lin) = v.get("lineage") {
                    if lin.is_string() {
                        if let Some(lin_str) = lin.as_str() {
                            lin_str.split(';').map(|s| s.trim().to_string()).collect()
                        } else {
                            vec![]
                        }
                    } else if lin.is_array() {
                        if let Some(arr) = lin.as_array() {
                            arr.iter()
                                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                .collect()
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                };
                // Walk lineage windows to find parent
                for names in lineage
                    .iter()
                    .rev()
                    .cloned()
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
                                tax_id: tax_id.clone(),
                                parent_tax_id: parent_tax_ids[0].clone(),
                                rank: rank.clone(),
                                scientific_name: Some(scientific_name.clone()),
                                names: Some(vec![Name {
                                    tax_id: tax_id.clone(),
                                    name: scientific_name.clone(),
                                    class: Some("scientific name".to_string()),
                                    unique_name: format!(
                                        "{}:{}",
                                        xref_label,
                                        scientific_name.to_case(Case::Lower)
                                    ),
                                    ..Default::default()
                                }]),
                                row_index: Some(row_index),
                                raw_row: Some(line.clone()),
                                ..Default::default()
                            };
                            existing_nodes.nodes.insert(tax_id.clone(), node);
                            match existing_nodes.children.entry(parent_tax_ids[0].clone()) {
                                Entry::Vacant(e) => {
                                    e.insert(vec![tax_id.clone()]);
                                }
                                Entry::Occupied(mut e) => {
                                    e.get_mut().push(tax_id.clone());
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
        // Always return empty, as ENA nodes are only attached via lookup
        Ok(Nodes { nodes, children })
    }

    pub fn from_genomehubs(
        genomehubs_files: PathBuf,
        options: &TaxonomyOptions,
        existing: Option<&mut Nodes>,
    ) -> Result<Nodes, crate::error::Error> {
        let (nodes, children) = if let Some(existing_nodes) = existing {
            (
                existing_nodes.nodes.clone(),
                existing_nodes.children.clone(),
            )
        } else {
            (HashMap::new(), HashMap::new())
        };
        let name_classes = &options.name_classes;
        let id_map = build_fast_lookup(
            &Nodes {
                nodes: nodes.clone(),
                children: children.clone(),
            },
            name_classes,
        );
        let (new_nodes, new_names, _source) = parse_file(
            genomehubs_files.clone(),
            &id_map,
            false,
            options.create_taxa,
            options.xref_label.clone(),
            false,
        )
        .map_err(crate::error::Error::from)?;
        // Try to add names to existing nodes
        let mut nodes_struct = Nodes { nodes, children };
        let _add_names_result = nodes_struct.add_names(&new_names);
        // Optionally, add new nodes if not present
        // let mut created_count = 0;
        for (taxid, node) in new_nodes.nodes.iter() {
            // Prevent self-parenting
            if node.tax_id == node.parent_tax_id {
                continue;
            }
            // Prevent cycles: check if parent is a descendant of this node
            let mut ancestor = node.parent_tax_id.clone();
            let mut cycle = false;
            while let Some(parent_node) = nodes_struct.nodes.get(&ancestor) {
                if parent_node.tax_id == node.tax_id {
                    cycle = true;
                    break;
                }
                if parent_node.tax_id == parent_node.parent_tax_id {
                    break;
                }
                ancestor = parent_node.parent_tax_id.clone();
            }
            if cycle {
                continue;
            }
            if !nodes_struct.nodes.contains_key(taxid) {
                nodes_struct.nodes.insert(taxid.clone(), node.clone());
                let parent = node.parent_tax_id.clone();
                let child = node.tax_id.clone();
                if parent != child {
                    match nodes_struct.children.entry(parent) {
                        std::collections::hash_map::Entry::Vacant(e) => {
                            e.insert(vec![child.clone()]);
                        }
                        std::collections::hash_map::Entry::Occupied(mut e) => {
                            if !e.get().contains(&child) {
                                e.get_mut().push(child.clone());
                            }
                        }
                    }
                }
                // created_count += 1;
            }
        }
        Ok(nodes_struct)
    }

    /// Efficiently merge only names from new_nodes into self, skipping parent/child/cycle logic.
    /// Use this when create_taxa is false for large taxonomies (e.g. OTT) to avoid O(N^2) merge cost.
    pub fn merge_names_only(&mut self, new_nodes: &Nodes) -> Result<(), anyhow::Error> {
        let mut name_map: HashMap<String, Vec<Name>> = HashMap::new();
        for (_, node) in &new_nodes.nodes {
            if let Some(names) = &node.names {
                // find a name with unique_name starting with "ncbi:" and use that as the key
                if let Some(name) = names.iter().find(|n| n.unique_name.starts_with("ncbi:")) {
                    // remove "ncbi:" and use that as the key
                    let key = name.unique_name.trim_start_matches("ncbi:").to_string();
                    name_map.entry(key.clone()).or_insert_with(Vec::new).extend(
                        names.iter().cloned().map(|mut n| {
                            n.tax_id = key.clone();
                            n
                        }),
                    );
                }
            }
        }
        self.add_names(&name_map)
    }
}
