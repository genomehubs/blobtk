//!
//! Invoked by calling:
//! `blobtk taxonomy <args>`

use std::collections::HashSet;

use anyhow;

use crate::cli;
use crate::error;
use crate::io;

pub use cli::TaxonomyOptions;

use crate::parse::lookup::{build_fast_lookup, lookup_nodes};
use crate::parse::nodes::Nodes;

use crate::parse::parse_file;

fn load_options(options: &cli::TaxonomyOptions) -> Result<cli::TaxonomyOptions, error::Error> {
    if let Some(config_file) = options.config_file.clone() {
        let reader = match io::file_reader(config_file.clone()) {
            Ok(r) => r,
            Err(_) => {
                return Err(error::Error::FileNotFound(format!(
                    "{}",
                    &config_file.to_str().unwrap()
                )))
            }
        };
        let taxonomy_options: cli::TaxonomyOptions = match serde_yaml::from_reader(reader) {
            Ok(options) => options,
            Err(err) => {
                return Err(error::Error::SerdeError(format!(
                    "{} {}",
                    &config_file.to_str().unwrap(),
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
        nodes = match options.taxonomy_format {
            Some(cli::TaxonomyFormat::GBIF) => {
                Nodes::from_gbif(taxdump, &options, existing).unwrap()
            }
            Some(cli::TaxonomyFormat::ENA) => {
                Nodes::from_jsonl(taxdump, &options, existing).unwrap()
            }
            _ => Nodes::from_taxdump(taxdump, options.xref_label.clone()).unwrap(),
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
        for taxonomy in taxonomies {
            let new_nodes = taxdump_to_nodes(&taxonomy, Some(&mut nodes)).unwrap();
            // Only run lookup_nodes for non-ENA taxonomies
            if let Some(taxonomy_format) = taxonomy.taxonomy_format {
                if !matches!(taxonomy_format, cli::TaxonomyFormat::ENA) {
                    lookup_nodes(
                        &new_nodes,
                        &mut nodes,
                        &taxonomy.name_classes,
                        &options.name_classes,
                        taxonomy.xref_label.clone(),
                        taxonomy.create_taxa,
                    );
                }
            }
            nodes.merge(&new_nodes)?;
        }
    }

    if let Some(genomehubs_files) = options.genomehubs_files.clone() {
        let id_map = build_fast_lookup(&nodes, &options.name_classes);
        for genomehubs_file in genomehubs_files {
            let (new_nodes, new_names, source) = parse_file(genomehubs_file, &id_map, false)?;
            nodes.add_names(&new_names)?;
            nodes.merge(&new_nodes)?;
        }
    }

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
