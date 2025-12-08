use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::ffi::CString;
use std::io::Write;
use std::path::PathBuf;

use blart::TreeMap;
use indicatif::ProgressBar;
use serde::{Deserialize, Deserializer, Serialize};

/// Functions for name lookup.
pub mod lookup;

/// Functions for handling names and nodes
pub mod nodes;

/// Functions for handling GenomeHubs configuration files
pub mod genomehubs;

use crate::error;

use genomehubs::{
    GHubsConfig, SkipPartial, Source, StringOrVec, ValidationCounts, ValidationStatus,
};
use lookup::{
    clean_name, match_taxonomy_section, Candidate, MatchCounts, MatchStatus, TaxonInfo, TaxonMatch,
};
use nodes::{Name, Node, Nodes};

// Add new names to the taxonomy
fn add_new_names(
    taxon: &Candidate,
    taxon_names: &HashMap<String, String>,
    names: &mut HashMap<String, Vec<Name>>,
    id_map: &TreeMap<CString, Vec<TaxonInfo>>,
    xref_label: &Option<String>,
) {
    if !taxon.tax_id.is_some() {
        return;
    }
    let tax_id = taxon.tax_id.clone().unwrap();
    for (name_class, name) in taxon_names.iter() {
        if name == "None" || name == "NA" || name.is_empty() {
            continue;
        }
        // does name already exist in id_map associated with the same class and taxid?
        // if so, skip for now
        if let Some(tax_info) = id_map.get(&CString::new(clean_name(&name)).unwrap()) {
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

        let unique_name = match xref_label {
            Some(label) => format!("{}:{}", label, name),
            None => name.clone(),
        };
        let taxon_name = Name {
            tax_id: tax_id.clone(),
            name: name.clone(),
            unique_name,
            class: Some(name_class.replace('_', " ")),
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
    _id_map: &TreeMap<CString, Vec<TaxonInfo>>,
    row_index: Option<usize>,
    raw_row: Option<String>,
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
                row_index,
                raw_row,
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
    create_taxa: bool,
    xref_label: Option<String>,
    skip_tsv: bool,
) -> Result<(HashMap<String, Vec<Name>>, HashMap<String, Node>), error::Error> {
    let keys = vec!["attributes", "taxon_names", "taxonomy"];
    let mut fixed_names = HashMap::new();
    ghubs_config.init_csv_reader(Some(keys.clone()), skip_tsv)?;
    ghubs_config.init_file_writers(write_validated, true);
    if !id_map.is_empty() {
        ghubs_config.init_taxon_id();
        fixed_names = ghubs_config.init_taxon_names();
    }

    let mut names = HashMap::new();
    let mut nodes = HashMap::new();

    let mut validation_counts: ValidationCounts = ValidationCounts::default();
    let mut match_counts = MatchCounts::default();

    let pb = ProgressBar::new_spinner();

    for (row_index, result) in ghubs_config
        .init_csv_reader(None, skip_tsv)?
        .records()
        .enumerate()
    {
        pb.set_message(format!("[+] {}", validation_counts.to_jsonl().as_str()));
        pb.inc(1);
        if let Err(err) = result {
            let err: error::Error = err.into();
            ghubs_config.handle_error(&err, row_index);
            continue;
        }
        let record = result?;
        let raw_row = record.iter().collect::<Vec<_>>().join("\t");
        let (mut processed, mut combined_report) =
            ghubs_config.validate_record(&record, row_index, &keys);
        validation_counts.update(&combined_report.counts);
        if combined_report.status == ValidationStatus::Partial {
            if ghubs_config.file.as_ref().unwrap().skip_partial == Some(SkipPartial::Row) {
                continue;
            }
        }

        let taxonomy_section = processed.get(&"taxonomy".to_string());

        if taxonomy_section.is_none() || id_map.is_empty() {
            ghubs_config.write_processed_row(&processed)?;
            continue;
        }

        if let Some(tax_section) = taxonomy_section {
            if tax_section.get("taxon_id").is_none() {
                let mut taxon_id_section = tax_section.clone();
                taxon_id_section.insert("taxon_id".to_string(), "None".to_string());
                // replace taxonomy section with new section
                processed.insert("taxonomy".to_string(), taxon_id_section);
            }
        }
        let taxonomy_section = processed.get(&"taxonomy".to_string());
        let taxon_names_section = processed.get(&"taxon_names".to_string());
        let (assigned_taxon, taxon_match) =
            match_taxonomy_section(taxonomy_section.unwrap(), id_map, Some(&fixed_names));
        let taxon_name = taxon_match.taxon.name.clone();
        // add taxon name to combined report
        combined_report.taxon_name = Some(taxon_name.clone());
        if let Some(taxon) = &assigned_taxon {
            match_counts.assigned += 1;
            if let Some(taxon_names) = taxon_names_section {
                add_new_names(&taxon, taxon_names, &mut names, &id_map, &xref_label);
            }
            ghubs_config.write_modified_row(
                &processed,
                "taxonomy",
                "taxon_id".to_string(),
                taxon.tax_id.clone().unwrap(),
            )?;
        } else {
            match_counts.unassigned += 1;
        }
        let mut unmatched = false;
        if let Some(status) = taxon_match.rank_status.as_ref() {
            match status {
                MatchStatus::Match(_) => match_counts.id_match += 1,
                MatchStatus::MergeMatch(_) => match_counts.merge_match += 1,
                MatchStatus::Mismatch(_) => {
                    match_counts.mismatch += 1;
                    combined_report.status = ValidationStatus::Mismatch;
                    combined_report.mismatch.push(taxon_match.clone());
                    validation_counts.mismatch += 1;

                    ghubs_config.write_exception(&combined_report);
                }
                MatchStatus::MultiMatch(_) => {
                    match_counts.multimatch += 1;
                    combined_report.status = ValidationStatus::Multimatch;
                    combined_report.multimatch.push(taxon_match.clone());
                    validation_counts.multimatch += 1;

                    ghubs_config.write_exception(&combined_report);
                }
                MatchStatus::PutativeMatch(_) => {
                    match_counts.putative += 1;

                    if assigned_taxon.is_none() {
                        combined_report.status = ValidationStatus::Putative;
                        combined_report.putative.push(taxon_match.clone());
                        validation_counts.putative += 1;

                        ghubs_config.write_exception(&combined_report);
                    }
                }
                MatchStatus::None => {
                    match_counts.none += 1;
                    unmatched = true;
                    combined_report.status = ValidationStatus::Nomatch;
                    // combined_report.multimatch.push(taxon_match.clone());
                    validation_counts.nomatch += 1;

                    ghubs_config.write_exception(&combined_report);
                }
            }
        } else if let Some(_options) = &taxon_match.rank_options {
            match_counts.spellcheck += 1;
            validation_counts.spellcheck += 1;
            combined_report.status = ValidationStatus::Spellcheck;
            combined_report.spellcheck.push(taxon_match.clone());
            ghubs_config.write_exception(&combined_report);
        } else {
            match_counts.none += 1;
            unmatched = true;
            combined_report.status = ValidationStatus::Nomatch;
            // combined_report.multimatch.push(taxon_match.clone());
            validation_counts.nomatch += 1;

            ghubs_config.write_exception(&combined_report);
        }
        if unmatched && create_taxa {
            // Find/add parent genus first
            let mut parent_tax_id = None;
            let tax_section = taxonomy_section.unwrap();
            // Try to get genus from taxonomy_section or from species/subspecies name
            let genus_name = if let Some(genus) = tax_section.get("genus") {
                if !genus.is_empty() {
                    Some(genus.clone())
                } else {
                    None
                }
            } else if let Some(species) = tax_section.get("species") {
                species.split_whitespace().next().map(|s| s.to_string())
            } else if let Some(subspecies) = tax_section.get("subspecies") {
                subspecies.split_whitespace().next().map(|s| s.to_string())
            } else {
                None
            };

            // Try to find or create genus node
            if let Some(ref genus) = genus_name {
                // Look up genus in id_map
                let genus_tax_id = if let Some(genus_infos) =
                    id_map.get(&CString::new(clean_name(&genus)).unwrap())
                {
                    // Use first match if available
                    genus_infos.first().map(|info| info.tax_id.clone())
                } else {
                    None
                };
                if let Some(gtid) = genus_tax_id {
                    parent_tax_id = Some(gtid);
                } else {
                    // Create new genus node
                    let genus_tax_id = format!("anc_{}", genus);
                    // Set parent_tax_id to higher taxon match if available, else root
                    let genus_parent_tax_id = match &taxon_match.higher_status {
                        Some(MatchStatus::Match(parent))
                        | Some(MatchStatus::MergeMatch(parent))
                        | Some(MatchStatus::PutativeMatch(parent)) => {
                            parent.tax_id.clone().unwrap_or_else(|| "1".to_string())
                        }
                        _ => "1".to_string(),
                    };
                    let genus_node = Node {
                        tax_id: genus_tax_id.clone(),
                        parent_tax_id: genus_parent_tax_id,
                        rank: "genus".to_string(),
                        scientific_name: Some(genus.clone()),
                        names: Some(vec![Name {
                            tax_id: genus_tax_id.clone(),
                            name: genus.clone(),
                            unique_name: genus.clone(),
                            class: Some("scientific name".to_string()),
                            ..Default::default()
                        }]),
                        row_index: Some(row_index),
                        raw_row: Some(raw_row.clone()),
                        ..Default::default()
                    };
                    nodes.insert(genus_tax_id.clone(), genus_node);
                    parent_tax_id = Some(genus_tax_id);
                }
            }

            // Now create the species/subspecies node, using genus as parent if found/created
            let mut new_taxon_match = taxon_match.clone();
            if let Some(ref parent_id) = parent_tax_id {
                // Set higher_status to point to genus
                new_taxon_match.higher_status = Some(MatchStatus::PutativeMatch(Candidate {
                    tax_id: Some(parent_id.clone()),
                    rank: "genus".to_string(),
                    name: genus_name.clone().unwrap_or_default(),
                    anc_ids: None,
                }));
            }

            if let Some(node) = add_new_taxid(
                &new_taxon_match,
                tax_section,
                &id_map,
                Some(row_index),
                Some(raw_row.clone()),
            ) {
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
                        &xref_label,
                    );
                }
                ghubs_config.write_modified_row(
                    &processed,
                    "taxonomy",
                    "taxon_id".to_string(),
                    node.tax_id.clone(),
                )?;
                // TODO: add new taxid to id_map and increment counter
            }
        }
    }
    pb.finish_with_message(format!("done"));
    println!("Validation Report: {}", validation_counts.to_jsonl());
    if write_validated {
        // write ghubs_config back to file in validated directory
        write_updated_config(config_file, ghubs_config, keys);
    }

    println!("Taxon Assignment Report: {}", match_counts.to_jsonl());
    Ok((names, nodes))
}

fn write_updated_config(config_file: &PathBuf, ghubs_config: &mut GHubsConfig, keys: Vec<&str>) {
    let mut new_config_file = config_file.clone();
    // get file name
    let config_file_name = config_file.file_name().unwrap().to_str().unwrap();
    new_config_file.pop();
    new_config_file.push("validated");
    std::fs::create_dir_all(&new_config_file).unwrap();
    new_config_file.push(config_file_name);
    for key in keys.iter() {
        if ghubs_config.get(key).is_some() {
            for (field, value) in ghubs_config.get_mut(key).unwrap().iter_mut() {
                value.header = Some(StringOrVec::Single(field.clone()));
            }
        }
    }

    let mut file = std::fs::File::create(&new_config_file).unwrap();
    // write ghubs_config YAML to file
    file.write_all(serde_yaml::to_string(&ghubs_config).unwrap().as_bytes())
        .unwrap();
}

pub fn parse_file(
    config_file: PathBuf,
    id_map: &TreeMap<CString, Vec<TaxonInfo>>,
    write_validated: bool,
    create_taxa: bool,
    xref_label: Option<String>,
    skip_tsv: bool,
) -> Result<(Nodes, HashMap<String, Vec<Name>>, Source), error::Error> {
    // let mut children = HashMap::new();

    let mut ghubs_config = match GHubsConfig::new(&config_file) {
        Ok(ghubs_config) => ghubs_config,
        Err(err) => return Err(err),
    };
    // let source = Source::new(&ghubs_config);
    let (names, tmp_nodes) = nodes_from_file(
        &config_file,
        &mut ghubs_config,
        &id_map,
        write_validated,
        create_taxa,
        xref_label.clone(),
        skip_tsv,
    )?;
    let mut nodes = Nodes {
        nodes: HashMap::new(),
        children: HashMap::new(),
    };
    let source = Source::new(&ghubs_config);
    for (tax_id, node) in tmp_nodes.iter() {
        let mut node = node.clone();
        let unique_name = match &xref_label {
            Some(label) => format!(
                "{}:{}",
                label,
                node.scientific_name.clone().unwrap_or_default()
            ),
            None => String::new(),
        };
        let name = Name {
            tax_id: tax_id.clone(),
            name: node.scientific_name.clone().unwrap(),
            unique_name,
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
