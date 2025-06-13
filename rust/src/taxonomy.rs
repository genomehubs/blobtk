//!
//! Invoked by calling:
//! `blobtk taxonomy <args>`

use std::collections::HashSet;

use anyhow;

use crate::cli;
use crate::error;
use crate::io;

pub use cli::TaxonomyOptions;

use crate::parse::lookup::{build_fast_lookup, lookup_nodes, lookup_nodes_by_id};
use crate::parse::nodes::Nodes;

use crate::parse::parse_file;

fn load_options(options: &cli::TaxonomyOptions) -> Result<cli::TaxonomyOptions, error::Error> {
    if let Some(config_file) = options.config_file.clone() {
        let reader = match io::file_reader(config_file.clone()) {
            Ok(r) => r,
            Err(_) => {
                return Err(error::Error::FileNotFound(format!(
                    "{}",
                    &config_file.to_string_lossy()
                )))
            }
        };
        let taxonomy_options: cli::TaxonomyOptions = match serde_yaml::from_reader(reader) {
            Ok(options) => options,
            Err(err) => {
                return Err(error::Error::SerdeError(format!(
                    "{} {}",
                    &config_file.to_string_lossy(),
                    err.to_string()
                )))
            }
        };
        return Ok(TaxonomyOptions {
            path: match taxonomy_options.path {
                Some(path) => Some(path),
                None => options.path.clone(),
            },
            taxonomy_format: match taxonomy_options.taxonomy_format {
                Some(taxonomy_format) => Some(taxonomy_format),
                None => options.taxonomy_format.clone(),
            },
            root_taxon_id: match taxonomy_options.root_taxon_id {
                Some(root_taxon_id) => Some(root_taxon_id),
                None => options.root_taxon_id.clone(),
            },
            leaf_taxon_id: match taxonomy_options.leaf_taxon_id {
                Some(leaf_taxon_id) => Some(leaf_taxon_id),
                None => options.leaf_taxon_id.clone(),
            },
            base_taxon_id: match taxonomy_options.base_taxon_id {
                Some(base_taxon_id) => Some(base_taxon_id),
                None => options.base_taxon_id.clone(),
            },
            out: match taxonomy_options.out {
                Some(out) => Some(out),
                None => options.out.clone(),
            },
            xref_label: match taxonomy_options.xref_label {
                Some(xref_label) => Some(xref_label),
                None => options.xref_label.clone(),
            },
            name_classes: if taxonomy_options.name_classes.len() > 0 {
                taxonomy_options.name_classes.clone()
            } else {
                options.name_classes.clone()
            },
            create_taxa: taxonomy_options.create_taxa.clone(),
            taxonomies: taxonomy_options.taxonomies.clone(),
            genomehubs_files: match taxonomy_options.genomehubs_files {
                Some(genomehubs_files) => Some(genomehubs_files),
                None => options.genomehubs_files.clone(),
            },

            ..Default::default()
        });
    }
    Ok(options.clone())
}

pub fn taxdump_to_nodes(
    options: &cli::TaxonomyOptions,
    existing: Option<&mut Nodes>,
) -> Result<Nodes, error::Error> {
    let options = load_options(&options)?;
    let nodes;
    if let Some(taxdump) = options.path.clone() {
        dbg!(&taxdump);
        nodes = match options.taxonomy_format {
            Some(cli::TaxonomyFormat::GBIF) => Nodes::from_gbif(taxdump, &options, existing)?,
            Some(cli::TaxonomyFormat::ENA) => Nodes::from_jsonl(taxdump, &options, existing)?,
            Some(cli::TaxonomyFormat::OTT) => Nodes::from_ott(taxdump, &options, existing)?,
            Some(cli::TaxonomyFormat::GenomeHubs) => {
                Nodes::from_genomehubs(taxdump, &options, existing)?
            }
            _ => Nodes::from_taxdump(taxdump, options.xref_label.clone())?,
        };
    } else {
        return Err(error::Error::NotDefined(format!("taxdump")));
    }
    Ok(nodes)
}

/// Execute the `taxonomy` subcommand from `blobtk`.
pub fn taxonomy(options: &cli::TaxonomyOptions) -> Result<(), anyhow::Error> {
    let options = load_options(&options)?;
    // 1. Parse the base taxonomy (main path)
    let mut nodes = taxdump_to_nodes(&options, None)?;

    // 2. Merge in each additional taxonomy in the order given in the config
    if let Some(taxonomies) = options.taxonomies.clone() {
        for taxonomy_options in taxonomies {
            let new_nodes = taxdump_to_nodes(&taxonomy_options, Some(&mut nodes))?;
            let taxonomy_format = taxonomy_options.taxonomy_format;
            let mut filtered_new_nodes = new_nodes.clone();
            // Filter new_nodes by root_taxon_id and base_taxon_id if specified
            if let Some(root_ids) = taxonomy_options.root_taxon_id.clone() {
                let mut keep = std::collections::HashSet::new();
                for root_id in root_ids {
                    // Collect all descendants of root_id
                    let mut stack = vec![root_id.clone()];
                    while let Some(tid) = stack.pop() {
                        if keep.insert(tid.clone()) {
                            if let Some(children) = filtered_new_nodes.children.get(&tid) {
                                for child in children {
                                    stack.push(child.clone());
                                }
                            }
                        }
                    }
                }
                filtered_new_nodes.nodes.retain(|k, _| keep.contains(k));
                filtered_new_nodes.children.retain(|k, _| keep.contains(k));
            }
            // Optionally filter by base_taxon_id (if you want to restrict further)
            if let Some(base_id) = taxonomy_options.base_taxon_id.clone() {
                if filtered_new_nodes.nodes.contains_key(&base_id) {
                    let mut keep = std::collections::HashSet::new();
                    let mut stack = vec![base_id.clone()];
                    while let Some(tid) = stack.pop() {
                        if keep.insert(tid.clone()) {
                            if let Some(children) = filtered_new_nodes.children.get(&tid) {
                                for child in children {
                                    stack.push(child.clone());
                                }
                            }
                        }
                    }
                    filtered_new_nodes.nodes.retain(|k, _| keep.contains(k));
                    filtered_new_nodes.children.retain(|k, _| keep.contains(k));
                }
            }
            match taxonomy_format {
                Some(cli::TaxonomyFormat::GBIF) => {
                    lookup_nodes(
                        &filtered_new_nodes,
                        &mut nodes,
                        &taxonomy_options.name_classes,
                        &options.name_classes,
                        taxonomy_options.xref_label.clone(),
                        taxonomy_options.create_taxa,
                    );
                }
                Some(cli::TaxonomyFormat::NCBI) => {
                    lookup_nodes(
                        &filtered_new_nodes,
                        &mut nodes,
                        &taxonomy_options.name_classes,
                        &options.name_classes,
                        taxonomy_options.xref_label.clone(),
                        taxonomy_options.create_taxa,
                    );
                }
                Some(cli::TaxonomyFormat::OTT) => {
                    lookup_nodes_by_id(
                        &filtered_new_nodes,
                        &mut nodes,
                        &"ncbi",
                        taxonomy_options.xref_label.clone(),
                        taxonomy_options.create_taxa,
                    );
                }
                _ => {
                    // skip lookup
                }
            }
            nodes.merge(&filtered_new_nodes)?;
        }
    }

    // if let Some(genomehubs_files) = options.genomehubs_files.clone() {
    //     let id_map = build_fast_lookup(&nodes, &options.name_classes);
    //     for genomehubs_file in genomehubs_files {
    //         let (new_nodes, new_names, source) =
    //             parse_file(genomehubs_file, &id_map, false, false, taxonomy.xref_label)?;
    //         nodes.add_names(&new_names)?;
    //         nodes.merge(&new_nodes)?;
    //     }
    // }

    if let Some(taxdump_out) = options.out.clone() {
        let root_taxon_ids = options.root_taxon_id.clone();
        let leaf_taxon_ids = options
            .leaf_taxon_id
            .clone()
            .map(|ids| ids.into_iter().collect::<HashSet<_>>());
        let base_taxon_id = options.base_taxon_id.clone();
        nodes.write_taxdump(
            root_taxon_ids,
            leaf_taxon_ids,
            base_taxon_id,
            &taxdump_out,
            false,
        );
    }
    Ok(())
}
