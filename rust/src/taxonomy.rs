//!
//! Invoked by calling:
//! `blobtk taxonomy <args>`

use anyhow;
use svg::node;

// use std::time::{Duration, Instant};

use crate::cli;
use crate::error;
use crate::io;

/// Functions for ncbi taxonomy processing.
pub mod parse;

/// Functions for name lookup.
pub mod lookup;

pub use cli::TaxonomyOptions;

pub use lookup::{build_fast_lookup, build_lookup, lookup_nodes};

use self::parse::{parse_ena_jsonl, parse_file, parse_gbif, parse_taxdump, write_taxdump, Nodes};

// use std::error::Error;
// use csv::Reader;

// fn example() -> Result<(), Box<dyn Error>> {
//     let mut rdr = Reader::from_path("foo.csv")?;
//     for result in rdr.records() {
//         let record = result?;
//         println!("{:?}", record);
//     }
//     Ok(())
// }

fn load_options(options: &cli::TaxonomyOptions) -> Result<cli::TaxonomyOptions, error::Error> {
    if let Some(config_file) = options.config_file.clone() {
        let reader = match io::file_reader(config_file.clone()) {
            Some(r) => r,
            None => {
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
            Some(cli::TaxonomyFormat::GBIF) => parse_gbif(taxdump).unwrap(),
            Some(cli::TaxonomyFormat::ENA) => parse_ena_jsonl(taxdump, existing).unwrap(),
            _ => parse_taxdump(taxdump, options.xref_label.clone()).unwrap(),
        };
    } else {
        return Err(error::Error::NotDefined(format!("taxdump")));
    }
    Ok(nodes)
}

/// Execute the `taxonomy` subcommand from `blobtk`.
pub fn taxonomy(options: &cli::TaxonomyOptions) -> Result<(), anyhow::Error> {
    let options = load_options(&options)?;
    let mut nodes = taxdump_to_nodes(&options, None)?;
    // if let Some(taxdump) = options.path.clone() {
    //     nodes = match options.taxonomy_format {
    //         Some(cli::TaxonomyFormat::NCBI) => parse_taxdump(taxdump)?,
    //         Some(cli::TaxonomyFormat::GBIF) => parse_gbif(taxdump)?,
    //         None => Nodes {
    //             ..Default::default()
    //         },
    //     };
    //     if let Some(taxdump_out) = options.out.clone() {
    //         let root_taxon_ids = options.root_taxon_id.clone();
    //         let base_taxon_id = options.base_taxon_id.clone();
    //         write_taxdump(&nodes, root_taxon_ids, base_taxon_id, taxdump_out);
    //     }
    // }

    if let Some(taxonomies) = options.taxonomies.clone() {
        for taxonomy in taxonomies {
            let new_nodes = taxdump_to_nodes(&taxonomy, Some(&mut nodes)).unwrap();
            // match new_nodes to nodes
            if let Some(taxonomy_format) = taxonomy.taxonomy_format {
                if matches!(taxonomy_format, cli::TaxonomyFormat::ENA) {
                    continue;
                }

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
    }

    if let Some(genomehubs_files) = options.genomehubs_files.clone() {
        let id_map = build_fast_lookup(&nodes, &options.name_classes);
        dbg!(nodes.nodes.len());
        for genomehubs_file in genomehubs_files {
            // match taxa to nodes
            // todo: add support for multiple genomehubs files
            let (new_nodes, new_names, source) = parse_file(genomehubs_file, &id_map, false)?;
            // add new nodes to existing nodes
            dbg!(new_nodes.nodes.len());
            nodes.add_names(&new_names)?;
            nodes.merge(&new_nodes)?;
        }
        dbg!(nodes.nodes.len());
    }

    if let Some(taxdump_out) = options.out.clone() {
        let root_taxon_ids = options.root_taxon_id.clone();
        let base_taxon_id = options.base_taxon_id.clone();
        write_taxdump(&nodes, root_taxon_ids, base_taxon_id, taxdump_out);
    }

    // if let Some(gbif_backbone) = options.gbif_backbone.clone() {
    //     // let trie = build_trie(&nodes);
    //     if let Ok(gbif_nodes) = parse_gbif(gbif_backbone) {
    //         println!("{}", gbif_nodes.nodes.len());
    //         if let Some(taxdump_out) = options.taxdump_out.clone() {
    //             let root_taxon_ids = options.root_taxon_id.clone();
    //             let base_taxon_id = options.base_taxon_id.clone();
    //             write_taxdump(&gbif_nodes, root_taxon_ids, base_taxon_id, taxdump_out);
    //         }
    //     }
    // }

    // if let Some(data_dir) = options.data_dir.clone() {
    //     let trie = build_trie(&nodes);
    //     let rank = "genus".to_string();
    //     let higher_rank = "family".to_string();
    //     let start = Instant::now();
    //     dbg!(trie.predictive_search(vec![
    //         rank,
    //         "arabidopsis".to_string(),
    //         higher_rank,
    //         "brassicaceae".to_string()
    //     ]));
    //     let duration = start.elapsed();

    //     println!("Time elapsed in expensive_function() is: {:?}", duration);
    // }
    // TODO: make lookup case insensitive
    // TODO: add support for synonym matching
    // TODO: read in taxon names from additonal files
    // TODO: add support for fuzzy matching?
    // TODO: hang additional taxa on the loaded taxonomy
    Ok(())
}
