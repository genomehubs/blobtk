use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::io::Write;
use std::path::PathBuf;

use anyhow;
use convert_case::{Case, Casing};
use nom::{
    bytes::complete::{tag, take_until},
    combinator::map,
    multi::separated_list0,
    IResult,
};

use struct_iterable::Iterable;

use crate::cli::TaxonomyOptions;
use crate::io;

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
                } else {
                    values.push("".to_string());
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
    /// Create a new Nodes struct.
    pub fn new() -> Nodes {
        Nodes {
            nodes: HashMap::new(),
            children: HashMap::new(),
        }
    }

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
        root_taxon_ids: Option<Vec<String>>,
        leaf_taxon_ids: Option<HashSet<String>>,
        base_id: Option<String>,
        taxdump_path: &PathBuf,
        append: bool,
    ) -> () {
        let nodes_path = io::append_to_path(taxdump_path, "/nodes.dmp");
        let names_path = io::append_to_path(taxdump_path, "/names.dmp");

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

        let mut root_ids = vec![];
        match root_taxon_ids {
            Some(ids) => {
                for id in ids {
                    root_ids.push(id)
                }
            }
            None => root_ids.push("1".to_string()),
        };

        let mut ancestors = HashSet::new();
        for root_id in root_ids {
            if let Some(lineage_root_id) = base_id.clone() {
                let lineage = self.lineage(&lineage_root_id, &root_id);
                for anc_node in lineage {
                    if !ancestors.contains(&anc_node.tax_id.clone()) {
                        writeln!(nodes_writer, "{}", &anc_node).unwrap();
                        nodes_writer.flush().unwrap();
                        if let Some(names) = anc_node.names.as_ref() {
                            for name in names {
                                writeln!(names_writer, "{}", &name).unwrap();
                                names_writer.flush().unwrap();
                            }
                        }
                        ancestors.insert(anc_node.tax_id.clone());
                    }
                }
            }
            if let Some(root_node) = self.nodes.get(&root_id) {
                writeln!(nodes_writer, "{}", &root_node).unwrap();
                nodes_writer.flush().unwrap();
                if let Some(names) = root_node.names.as_ref() {
                    for name in names {
                        writeln!(names_writer, "{}", &name).unwrap();
                        names_writer.flush().unwrap();
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
                        self.write_taxdump(
                            Some(vec![child.clone()]),
                            leaf_taxon_ids.clone(),
                            None,
                            taxdump_path,
                            true,
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

    pub fn merge(&mut self, new_nodes: &Nodes) -> Result<(), anyhow::Error> {
        println!(
            "[DEBUG] merge: before: {}, merging: {}",
            self.nodes.len(),
            new_nodes.nodes.len()
        );
        let nodes = &mut self.nodes;
        let children = &mut self.children;
        for node in new_nodes.nodes.iter() {
            // Always insert/replace the node
            nodes.insert(node.1.tax_id.clone(), node.1.clone());
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
        println!("[DEBUG] merge: after: {}", self.nodes.len());
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

    /// Parse a taxdump directory into a Nodes struct.
    pub fn from_taxdump(
        taxdump: PathBuf,
        xref_label: Option<String>,
    ) -> Result<Nodes, anyhow::Error> {
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

    pub fn from_gbif(
        gbif_backbone: PathBuf,
        options: &TaxonomyOptions,
        existing: Option<&mut Nodes>,
    ) -> Result<Nodes, anyhow::Error> {
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
        }
        Ok(Nodes { nodes, children })
    }

    pub fn from_jsonl(
        jsonl_path: PathBuf,
        options: &TaxonomyOptions,
        existing: Option<&mut Nodes>,
    ) -> Result<Nodes, anyhow::Error> {
        use convert_case::Case;
        use convert_case::Casing;
        use serde_json::Value;
        use std::collections::hash_map::Entry;
        use std::fs::File;
        use std::io::{BufRead, BufReader};
        let xref_label = options
            .xref_label
            .clone()
            .unwrap_or_else(|| "ena".to_string());
        let name_classes = vec!["scientific name".to_string()];
        let mut nodes = HashMap::new();
        let mut children = HashMap::new();
        if let Some(existing_nodes) = existing {
            let table = crate::parse::lookup::build_lookup(existing_nodes, &name_classes, false);
            let file = File::open(jsonl_path)?;
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let line = line?;
                let v: Value = serde_json::from_str(&line)?;
                let tax_id = v["taxId"].as_str().unwrap_or("").to_string();
                let rank = v["rank"].as_str().unwrap_or("").to_string();
                let scientific_name = v["scientificName"].as_str().unwrap_or("").to_string();
                // Parse lineage as Vec<String>
                let lineage: Vec<String> = if let Some(lin) = v.get("lineage") {
                    if lin.is_string() {
                        lin.as_str()
                            .unwrap()
                            .split(';')
                            .map(|s| s.trim().to_string())
                            .collect()
                    } else if lin.is_array() {
                        lin.as_array()
                            .unwrap()
                            .iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .collect()
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
}
