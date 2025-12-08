use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::ffi::CString;

use blart::TreeMap;
use serde::{Deserialize, Serialize};

use crate::parse::nodes::{Name, Node, Nodes};
use crate::utils::styled_progress_bar;

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
const HIGHER_RANKS: [&str; 5] = ["family", "order", "class", "phylum", "kingdom"];

pub fn build_lookup(
    nodes: &Nodes,
    name_classes: &Vec<String>,
    rank_letter: bool,
) -> HashMap<String, Vec<String>> {
    let mut table = HashMap::new();

    let rank_set: HashSet<&str> = HashSet::from_iter(RANKS.iter().cloned());
    let higher_rank_set: HashSet<&str> = HashSet::from_iter(HIGHER_RANKS.iter().cloned());
    let node_count = nodes.nodes.len();
    let progress_bar = styled_progress_bar(node_count, "Building lookup hash");

    for (tax_id, node) in nodes.nodes.iter() {
        progress_bar.inc(1);
        // if rank_set.contains(node.rank.as_str()) {
        let lineage = nodes.lineage(&"1".to_string(), tax_id);
        let names = node.names_by_class(Some(&name_classes), true).clone();
        for n in lineage.iter().rev() {
            let n_names = n.names_by_class(Some(&name_classes), true);
            for name in names.iter() {
                for n_name in n_names.iter() {
                    // if higher_rank_set.contains(n.rank.as_str()) {
                    let key = match rank_letter {
                        true => format!(
                            "{}:{}:{}:{}",
                            node.rank_letter(),
                            name,
                            n.rank_letter(),
                            n_name
                        ),
                        false => format!("{}:{}", name, n_name),
                    };
                    match table.entry(key) {
                        Entry::Vacant(e) => {
                            e.insert(vec![node.tax_id()]);
                        }
                        Entry::Occupied(mut e) => {
                            e.get_mut().push(node.tax_id());
                        }
                    }
                    // }
                }
            }
        }
        // }
    }
    progress_bar.finish();
    table
}

pub fn build_lineage_lookup(nodes: &Nodes, root_id: &String) -> HashMap<String, String> {
    let node_count = nodes.nodes.len();
    let progress_bar = styled_progress_bar(node_count, "Building lookup hash");
    let mut table = HashMap::new();

    for (tax_id, node) in nodes.nodes.iter() {
        progress_bar.inc(1);
        let lineage = nodes.lineage(root_id, tax_id);
        let s: String = lineage
            .iter()
            .map(|node| node.scientific_name())
            .collect::<Vec<String>>()
            .join("; ");
        let lineage_string = format!("{}; {}; ", s, node.scientific_name());
        table.insert(lineage_string, tax_id.clone());
    }
    progress_bar.finish();
    table
}

pub fn lookup_nodes(
    new_nodes: &Nodes,
    nodes: &mut Nodes,
    _new_name_classes: &Vec<String>,
    name_classes: &Vec<String>,
    xref_label: Option<String>,
    _create_taxa: bool,
) {
    let id_map = build_fast_lookup(&nodes, &name_classes);
    let node_count = new_nodes.nodes.len();
    let progress_bar = styled_progress_bar(node_count, "Looking up names");
    for node in new_nodes.nodes.values() {
        progress_bar.inc(1);
        let mut taxonomy_section = node.to_taxonomy_section(&new_nodes);
        let (assigned_taxon, _taxon_match) =
            match_taxonomy_section(&mut taxonomy_section, &id_map, None);
        if let Some(taxon) = assigned_taxon {
            let tax_id = taxon.tax_id.clone().unwrap();
            let new_tax_id = node.tax_id();
            let names = nodes
                .nodes
                .get_mut(&tax_id)
                .unwrap()
                .names
                .as_mut()
                .unwrap();
            let label = match xref_label {
                Some(ref l) => l.clone(),
                None => "".to_string(),
            };
            if let Some(new_names) = node.names.clone() {
                for name in new_names.iter() {
                    names.push(Name {
                        tax_id: tax_id.clone(),
                        unique_name: format!("{}:{}", &label, name.name.clone()),
                        ..name.clone()
                    });
                }
            }
            // Add the new node's tax_id as an xref
            let prefixed_name = format!("{}:{}", &label, new_tax_id);
            if !names
                .iter()
                .any(|n| n.name == new_tax_id && n.class.as_deref() == Some("xref"))
            {
                names.push(Name {
                    tax_id: tax_id.clone(),
                    name: prefixed_name.clone(),
                    unique_name: prefixed_name,
                    class: Some("xref".to_string()),
                });
            }
        }
    }
    progress_bar.finish();

    return;
}

#[derive(Clone, Debug, Default)]
pub struct TaxonInfo {
    pub tax_id: String,
    pub name: String,
    pub rank: String,
    pub anc_ids: HashSet<String>,
}

pub fn clean_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
        .trim()
        .to_string()
}

pub fn build_fast_lookup(
    nodes: &Nodes,
    name_classes: &Vec<String>,
) -> TreeMap<CString, Vec<TaxonInfo>> {
    let mut id_map: TreeMap<_, _> = TreeMap::new();

    let _rank_set: HashSet<&str> = HashSet::from_iter(RANKS.iter().cloned());
    let higher_rank_set: HashSet<&str> = HashSet::from_iter(HIGHER_RANKS.iter().cloned());
    let node_count = nodes.nodes.len();
    let progress_bar = styled_progress_bar(node_count, "Building lookup hash");
    for (tax_id, node) in nodes.nodes.iter() {
        progress_bar.inc(1);
        let lineage = nodes.lineage(&"1".to_string(), tax_id);
        let names = node.names_by_class(Some(&name_classes), true);
        let anc_ids: HashSet<String> = lineage
            .iter()
            .filter(|n| higher_rank_set.contains(n.rank.as_str()))
            .map(|n| n.tax_id())
            .collect();
        for name in names {
            let taxon_info = TaxonInfo {
                tax_id: tax_id.clone(),
                name: node.scientific_name(),
                rank: node.rank(),
                anc_ids: anc_ids.clone(),
            };
            let key = CString::new(clean_name(&name)).unwrap();
            match id_map.entry(key) {
                blart::map::Entry::Vacant(e) => {
                    e.insert(vec![taxon_info]);
                }
                blart::map::Entry::Occupied(mut e) => {
                    // Only insert if this tax_id is not already present
                    if !e.get().iter().any(|ti| ti.tax_id == taxon_info.tax_id) {
                        e.get_mut().push(taxon_info);
                    }
                }
            }
        }
    }
    progress_bar.finish();
    id_map
}

pub fn lookup_nodes_by_id(
    new_nodes: &Nodes,
    nodes: &mut Nodes,
    id_source: &str,
    xref_label: Option<String>,
    create_taxa: bool,
) {
    fn add_names_to_node(target_node: &mut Node, new_names: &[Name], xref_label: &str) {
        if let Some(names) = target_node.names.as_mut() {
            for name in new_names {
                // Avoid duplicate names
                if !names
                    .iter()
                    .any(|n| n.name == name.name && n.class == name.class)
                {
                    let mut new_name = name.clone();
                    // For xref, update unique_name to include xref_label
                    if new_name.class.as_deref() == Some("xref") {
                        new_name.unique_name = format!("{}:{}", xref_label, new_name.name);
                    }
                    // Always set the tax_id to the target node's tax_id
                    new_name.tax_id = target_node.tax_id.clone();
                    names.push(new_name);
                }
            }
        }
    }

    fn create_and_attach_taxon(
        nodes: &mut Nodes,
        new_node: &Node,
        hanger_id: &str,
        xref_label: &str,
    ) {
        let new_tax_id = format!("{}:{}", xref_label, new_node.tax_id());
        let mut new_node_clone = new_node.clone();
        new_node_clone.tax_id = new_tax_id.clone();
        new_node_clone.parent_tax_id = hanger_id.to_string();
        if let Some(names) = new_node_clone.names.as_mut() {
            for name in names.iter_mut() {
                name.tax_id = new_tax_id.clone();
            }
        }
        nodes.nodes.insert(new_tax_id.clone(), new_node_clone);
        match nodes.children.entry(hanger_id.to_string()) {
            Entry::Vacant(e) => {
                e.insert(vec![new_tax_id.clone()]);
            }
            Entry::Occupied(mut e) => {
                e.get_mut().push(new_tax_id.clone());
            }
        }
    }

    let label = xref_label.unwrap_or_else(|| id_source.to_string());
    let node_count = new_nodes.nodes.len();
    let progress_bar = styled_progress_bar(node_count, "Looking up xref IDs");
    for new_node in new_nodes.nodes.values() {
        progress_bar.inc(1);
        let mut matched = false;
        if let Some(ref new_names) = new_node.names {
            for name in new_names.iter() {
                if name.class.as_deref() == Some("xref") && name.name.starts_with(id_source) {
                    // Remove id_source: prefix
                    let id = name
                        .name
                        .strip_prefix(&format!("{}:", id_source))
                        .unwrap_or(&name.name);
                    if let Some(target_node) = nodes.nodes.get_mut(id) {
                        // Add all names from new_node to target_node, with xref_label
                        add_names_to_node(target_node, new_names, &label);
                        // Also add an xref name for the new_node's tax_id
                        let prefixed_name = format!("{}:{}", &label, new_node.tax_id());

                        if !target_node
                            .names
                            .as_ref()
                            .unwrap()
                            .iter()
                            .any(|n| n.name == prefixed_name && n.class.as_deref() == Some("xref"))
                        {
                            target_node.names.as_mut().unwrap().push(Name {
                                tax_id: id.to_string(),
                                name: prefixed_name.clone(),
                                unique_name: prefixed_name,
                                class: Some("xref".to_string()),
                                ..Default::default()
                            });
                        }
                        matched = true;
                        break;
                    }
                }
            }
        }
        if !matched && create_taxa {
            // Try to hang on parent if possible
            let hanger_id = new_node.parent_tax_id();
            if nodes.nodes.contains_key(&hanger_id) {
                create_and_attach_taxon(nodes, new_node, &hanger_id, &label);
            }
        }
    }
    progress_bar.finish();
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct Candidate {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tax_id: Option<String>,
    pub rank: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anc_ids: Option<HashSet<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum MatchStatus {
    Match(Candidate),
    MergeMatch(Candidate),
    Mismatch(Vec<Candidate>),
    MultiMatch(Vec<Candidate>),
    PutativeMatch(Candidate),
    #[default]
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaxonMatch {
    pub taxon: Candidate,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_status: Option<MatchStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_options: Option<Vec<Candidate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub higher_status: Option<MatchStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub higher_options: Option<Vec<Candidate>>,
}

impl TaxonMatch {
    pub fn to_json(&self) -> String {
        // summarise as json
        serde_json::to_string_pretty(&self).unwrap()
    }

    pub fn to_jsonl(&self) -> String {
        // summarise as jsonl
        serde_json::to_string(&self).unwrap()
    }
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MatchCounts {
    #[serde(skip_serializing_if = "is_zero")]
    pub assigned: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub unassigned: usize,
    #[serde(rename = "match", skip_serializing_if = "is_zero")]
    pub id_match: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub merge_match: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub mismatch: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub multimatch: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub putative: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub none: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub spellcheck: usize,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

impl MatchCounts {
    pub fn to_json(&self) -> String {
        // summarise as json
        serde_json::to_string_pretty(&self).unwrap()
    }

    pub fn to_jsonl(&self) -> String {
        // summarise as jsonl
        serde_json::to_string(&self).unwrap()
    }
}

fn check_higher_taxon(taxon: &Candidate, higher_taxon: &Candidate) -> bool {
    let higher_tax_id = higher_taxon.clone().tax_id.unwrap();
    if taxon.anc_ids.clone().unwrap().contains(&higher_tax_id) {
        true
    } else {
        false
    }
}

fn check_higher_rank(taxon: &Candidate, taxon_match: &TaxonMatch) -> bool {
    match taxon_match.higher_status.clone() {
        Some(MatchStatus::Match(_)) => true,
        Some(MatchStatus::MergeMatch(_)) => true,
        Some(MatchStatus::Mismatch(_)) => false,
        Some(MatchStatus::MultiMatch(higher_taxa)) => {
            // check that only one possible higher taxon matches the lineage
            higher_taxa
                .iter()
                .map(|higher_taxon| check_higher_taxon(taxon, &higher_taxon))
                .filter(|x| x.to_owned())
                .count()
                == 1
        }
        Some(MatchStatus::PutativeMatch(higher_taxon)) => check_higher_taxon(taxon, &higher_taxon),
        _ => {
            if let Some(higher_options) = taxon_match.higher_options.clone() {
                // check that only one possible higher taxon matches the lineage
                higher_options
                    .iter()
                    .map(|higher_taxon| check_higher_taxon(taxon, &higher_taxon))
                    .filter(|x| x.to_owned())
                    .count()
                    == 1
            } else {
                false
            }
        }
    }
}

fn set_ranks(taxonomy_section: &HashMap<String, String>) -> (Vec<String>, HashSet<&str>) {
    // Set ranks to check
    let mut ranks = vec![];
    if taxonomy_section.contains_key("taxon") {
        ranks.push("taxon".to_string());
    }
    for rank in RANKS.iter() {
        if taxonomy_section.contains_key(*rank) {
            ranks.push(rank.to_string());
        }
    }
    let lower_ranks: HashSet<&str> = RANKS[0..3].iter().cloned().collect();
    (ranks, lower_ranks)
}

fn update_taxon_id(
    taxonomy_section: &mut HashMap<String, String>,
    ranks: &Vec<String>,
    fixed_names: Option<&HashMap<String, HashMap<String, String>>>,
) {
    // Iterate over ranks
    for rank in ranks.iter() {
        let name = taxonomy_section.get(rank);
        if name.is_none() {
            continue;
        }

        // Check if name is in fixed_names
        let mut tax_id = None;
        if let Some(fixed_names) = fixed_names {
            if let Some(fixed) = fixed_names.get(rank) {
                if let Some(taxon_id) = fixed.get(&name.unwrap().clone()) {
                    tax_id = Some(taxon_id.clone());
                }
            }
        }
        // update taxon_id in taxonomy_section
        if let Some(tax_id) = tax_id {
            taxonomy_section.insert("taxon_id".to_string(), tax_id);
        }
        break;
    }
}

pub fn match_taxonomy_section(
    taxonomy_section: &HashMap<String, String>,
    id_map: &TreeMap<CString, Vec<TaxonInfo>>,
    fixed_names: Option<&HashMap<String, HashMap<String, String>>>,
) -> (Option<Candidate>, TaxonMatch) {
    let (ranks, lower_ranks) = set_ranks(taxonomy_section);
    let mut taxonomy_section = taxonomy_section.clone();
    update_taxon_id(&mut taxonomy_section, &ranks, fixed_names);

    // Check if taxon_id is present
    let mut taxon_id = taxonomy_section.get("taxon_id");
    if let Some(tax_id) = taxon_id {
        if tax_id == "None" {
            taxon_id = None;
        } else if let Some(ids) = id_map.get(&CString::new(tax_id.clone()).unwrap()) {
            if ids.len() == 1 {
                let taxon = Candidate {
                    tax_id: Some(ids[0].tax_id.clone()),
                    rank: ids[0].rank.clone(),
                    name: ids[0].name.clone(),
                    anc_ids: Some(ids[0].anc_ids.clone()),
                };
                return (
                    Some(taxon.clone()),
                    TaxonMatch {
                        taxon: taxon.clone(),
                        taxon_id: Some(ids[0].tax_id.clone()),
                        rank_status: Some(MatchStatus::Match(taxon.clone())),
                        ..Default::default()
                    },
                );
            }
        }
    }

    let mut taxon_match = TaxonMatch::default();
    // Iterate over ranks
    for (i, rank) in ranks.iter().enumerate() {
        let name = taxonomy_section.get(rank).unwrap().clone();
        let taxon = Candidate {
            name: name.clone(),
            tax_id: taxon_id.clone().map(|s| s.clone()),
            rank: rank.clone(),
            ..Default::default()
        };

        // Use first rank as taxon_match
        if i == 0 {
            taxon_match = TaxonMatch {
                taxon: taxon.clone(),
                ..Default::default()
            };
        } else if lower_ranks.contains(rank.as_str()) {
            continue;
        }

        match id_map.get(&CString::new(clean_name(&name)).unwrap()) {
            Some(ids) => {
                // Check if multiple matches
                if ids.len() > 1 {
                    let mut candidates = vec![];
                    for id in ids.iter() {
                        candidates.push(Candidate {
                            tax_id: Some(id.tax_id.clone()),
                            rank: id.rank.clone(),
                            name: id.name.clone(),
                            anc_ids: Some(id.anc_ids.clone()),
                        });
                    }
                    if i == 0 {
                        // Same rank as record
                        if let Some(tax_id) = taxon_match.clone().taxon.tax_id {
                            let mut has_match = false;

                            // Check if tax_id is in candidates
                            for candidate in candidates.iter() {
                                if tax_id == candidate.tax_id.clone().unwrap() {
                                    taxon_match.rank_status =
                                        Some(MatchStatus::Match(taxon.clone()));
                                    taxon_match.taxon_id = Some(candidate.tax_id.clone().unwrap());
                                    has_match = true;
                                    break;
                                }
                            }

                            // Check if tax_id is in merged IDs
                            if !has_match {
                                let id_matches = id_map.get(&CString::new(tax_id.clone()).unwrap());
                                if let Some(matches) = id_matches {
                                    if matches.len() == 1 {
                                        // Exact match to merged ID
                                        let merged_id = matches[0].tax_id.clone();
                                        for candidate in candidates.iter() {
                                            if merged_id == candidate.tax_id.clone().unwrap() {
                                                taxon_match.rank_status =
                                                    Some(MatchStatus::MergeMatch(Candidate {
                                                        tax_id: candidate.tax_id.clone(),
                                                        rank: candidate.rank.clone(),
                                                        name: candidate.name.clone(),
                                                        anc_ids: candidate.anc_ids.clone(),
                                                    }));
                                                taxon_match.taxon_id =
                                                    Some(candidate.tax_id.clone().unwrap());
                                                println!(
                                                    "Taxon {} has merged taxID {}",
                                                    taxon.name, merged_id
                                                );
                                                has_match = true;
                                                break;
                                            }
                                        }
                                    }

                                    // Mismatched taxon_id, possible namespace collision
                                    taxon_match.rank_status =
                                        Some(MatchStatus::Mismatch(candidates.clone()));
                                }
                                if !has_match {
                                    // Mismatched taxon_id, possible namespace collision
                                    taxon_match.rank_status =
                                        Some(MatchStatus::Mismatch(candidates.clone()));
                                }
                            }
                        } else {
                            // Multiple matches at same rank
                            taxon_match.rank_status = Some(MatchStatus::MultiMatch(candidates));
                        }
                    } else {
                        // Multiple matches at higher rank
                        taxon_match.higher_status = Some(MatchStatus::MultiMatch(candidates));
                    }
                } else {
                    // Single match found
                    // Use first match
                    let ids = ids.first().unwrap();
                    if i == 0 {
                        // Same rank as record
                        let filtered_id = taxon.tax_id.clone().filter(|s| !s.is_empty());
                        if let Some(ref tax_id) = filtered_id {
                            // has taxon ID
                            if tax_id.clone() == ids.tax_id {
                                // Exact match
                                taxon_match.rank_status = Some(MatchStatus::Match(taxon.clone()));
                                taxon_match.taxon_id = Some(ids.tax_id.clone());
                                break;
                            } else {
                                // Mismatched taxon_id, possible namespace collision
                                let id_matches = id_map.get(&CString::new(tax_id.clone()).unwrap());
                                let mut has_match = false;
                                if let Some(matches) = id_matches {
                                    if matches.len() == 1 && matches[0].tax_id == ids.tax_id {
                                        // Exact match to merged ID
                                        taxon_match.taxon_id = Some(matches[0].tax_id.clone());
                                        taxon_match.rank_status =
                                            Some(MatchStatus::MergeMatch(Candidate {
                                                tax_id: Some(ids.tax_id.clone()),
                                                rank: ids.rank.clone(),
                                                name: ids.name.clone(),
                                                anc_ids: Some(ids.anc_ids.clone()),
                                            }));
                                        has_match = true;
                                    }
                                }
                                if !has_match {
                                    // No match to merged ID
                                    taxon_match.rank_status =
                                        Some(MatchStatus::Mismatch(vec![Candidate {
                                            tax_id: Some(ids.tax_id.clone()),
                                            ..taxon.clone()
                                        }]));
                                }
                            }
                        } else {
                            // No taxon ID, putative match at same rank
                            taxon_match.rank_status = Some(MatchStatus::PutativeMatch(Candidate {
                                tax_id: Some(ids.tax_id.clone()),
                                anc_ids: Some(ids.anc_ids.clone()),
                                rank: ids.rank.clone(),
                                name: ids.name.clone(),
                            }));
                        }
                    } else {
                        // Putative match at higher rank
                        taxon_match = TaxonMatch {
                            higher_status: Some(MatchStatus::PutativeMatch(Candidate {
                                tax_id: Some(ids.tax_id.clone()),
                                rank: ids.rank.clone(),
                                name: ids.name.clone(),
                                anc_ids: Some(ids.anc_ids.clone()),
                            })),
                            ..taxon_match
                        };
                        break;
                    }
                }
            }
            None => {
                // Look for fuzzy matches
                let fuzzy: Vec<_> = id_map
                    .fuzzy(&CString::new(name.clone()).unwrap(), 2)
                    .collect();
                if fuzzy.len() > 0 {
                    // Check if fuzzy matches are at same rank
                    let mut candidates = vec![];
                    for fuzzies in fuzzy.iter() {
                        for f in fuzzies.1.iter() {
                            if i > 0 || f.rank == taxon_match.taxon.rank {
                                // Same rank as record or higher rank, add to candidates
                                candidates.push(Candidate {
                                    tax_id: Some(f.tax_id.clone()),
                                    rank: f.rank.clone(),
                                    name: f.name.clone(),
                                    anc_ids: Some(f.anc_ids.clone()),
                                });
                            }
                        }
                    }
                    if candidates.len() > 0 {
                        if i == 0 {
                            taxon_match.rank_options = Some(candidates);
                        } else {
                            taxon_match.higher_options = Some(candidates);
                        }
                    }
                }
            }
        }
    }

    let assigned_taxon;
    match taxon_match.rank_status.clone() {
        Some(MatchStatus::Match(taxon)) => {
            // println!("Taxon {} has taxID {}", taxon.name, taxon.tax_id.unwrap());
            assigned_taxon = Some(taxon);
        }
        Some(MatchStatus::MergeMatch(taxon)) => {
            // println!(
            //     "Taxon {} has merged taxID {}",
            //     taxon_match.taxon.name, taxon.tax_id.unwrap()
            // );
            assigned_taxon = Some(taxon);
        }
        Some(MatchStatus::Mismatch(_)) => {
            // println!(
            //     "Taxon {} has mismatched taxID, {} != {}",
            //     taxon_match.taxon.name,
            //     taxon_match.taxon.tax_id.clone().unwrap(),
            //     taxon.tax_id.unwrap()
            // );
            assigned_taxon = None;
        }
        Some(MatchStatus::MultiMatch(taxa)) => {
            // println!("Taxon {} has multiple matches", taxon_match.taxon.name);
            let mut candidates = vec![];
            for taxon in taxa.iter() {
                if check_higher_rank(&taxon, &taxon_match) {
                    candidates.push(taxon.clone());
                }
            }
            if candidates.len() == 1 {
                assigned_taxon = Some(candidates[0].clone());
            } else {
                assigned_taxon = None;
            }
        }
        Some(MatchStatus::PutativeMatch(taxon)) => {
            // println!(
            //     "Taxon {} has putative match to {}",
            //     taxon_match.taxon.name,
            //     taxon.clone().tax_id.unwrap()
            // );
            if check_higher_rank(&taxon, &taxon_match) {
                assigned_taxon = Some(taxon);
            } else {
                assigned_taxon = None;
            }
        }
        _ => {
            // if let Some(rank_options) = taxon_match.rank_options.clone() {
            //     for taxon in rank_options.iter() {
            //         // println!(
            //         //     "Taxon {} has potential match to {}, {}",
            //         //     taxon_match.taxon.name,
            //         //     taxon.name,
            //         //     taxon.tax_id.clone().unwrap()
            //         // );
            //         // check_higher_rank(&taxon, &taxon_match);
            //     }
            // }

            // println!("No match for taxon name {}", taxon_match.taxon.name);

            // TODO: create new taxon and add to id_map if no match
            assigned_taxon = None;
        }
    }
    (assigned_taxon, taxon_match)
}
