//!
//! Invoked by calling:
//! `blobtk taxonomy <args>`

use crate::parse::lookup::build_fast_lookup;
use crate::taxonomy::api::{run_api_server, TaxonomyService};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tokio::runtime::Runtime;

use anyhow;

use crate::cli::{self};
use crate::error;
use crate::io;

/// Functions running the taxonomy api
pub mod api;

pub use cli::TaxonomyOptions;

use crate::parse::lookup::{lookup_nodes, lookup_nodes_by_id};
use crate::parse::nodes::Nodes;

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
            output_format: match taxonomy_options.output_format {
                Some(output_format) => Some(output_format),
                None => taxonomy_options.output_format.clone(),
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
            api: options.api || taxonomy_options.api,
            port: if options.port != 3000 {
                options.port
            } else {
                taxonomy_options.port
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
            Some(cli::TaxonomyFormat::GBIF) => Nodes::from_gbif(taxdump, &options, existing)?,
            Some(cli::TaxonomyFormat::ENA) => Nodes::from_jsonl(taxdump, &options, existing)?,
            Some(cli::TaxonomyFormat::OTT) => Nodes::from_ott(taxdump, &options, existing)?,
            Some(cli::TaxonomyFormat::GenomeHubs) => {
                Nodes::from_genomehubs(taxdump, &options, existing)?
            }
            Some(cli::TaxonomyFormat::NCBI) => {
                Nodes::from_taxdump(taxdump, options.xref_label.clone())?
            }
            Some(cli::TaxonomyFormat::JSONL) => Nodes::new(),
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
    // If --api is set, start the API server and return
    if options.api {
        let is_ready = Arc::new(AtomicBool::new(false));
        let service = Arc::new(RwLock::new(TaxonomyService::empty()));
        let api_state = crate::taxonomy::api::ApiState {
            service: service.clone(),
            is_ready: is_ready.clone(),
        };

        let port = options.port;
        let api_handle = std::thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            let _ = rt.block_on(run_api_server(api_state, port)).unwrap();
        });

        let nodes = taxdump_to_nodes(&options, None)?;
        let id_map = build_fast_lookup(&nodes, &options.name_classes);

        {
            let mut svc = service.write().unwrap();
            svc.nodes = nodes;
            svc.id_map = id_map;
        }
        // All loading is done, so now set ready:
        is_ready.store(true, Ordering::SeqCst);

        // Join the API thread to block until it exits
        api_handle.join().unwrap();
        return Ok(());
    }
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
            // Use fast name-only merge for OTT if create_taxa is false
            if let Some(cli::TaxonomyFormat::OTT) = taxonomy_format {
                if taxonomy_options.create_taxa == false {
                    nodes.merge_names_only(&filtered_new_nodes)?;
                    continue;
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
            let merge_exceptions = nodes.merge(&filtered_new_nodes)?;
            // Write exceptions to exceptions.{taxonomyFormat}.json in the output directory
            if !merge_exceptions.is_empty() {
                use serde_json;
                use std::fs::OpenOptions;
                use std::io::Write;
                let out_dir = options
                    .out
                    .as_ref()
                    .map(|p| p.clone())
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                // ensure the output directory exists
                std::fs::create_dir_all(&out_dir).expect("Unable to create output directory");
                let format_str = taxonomy_format
                    .as_ref()
                    .map(|f| format!("{}", f).to_lowercase())
                    .unwrap_or_else(|| "unknown".to_string());
                let exceptions_path = out_dir.join(format!("exceptions.{}.jsonl", format_str));
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&exceptions_path)
                    .expect("Unable to open exceptions file");
                for exception in &merge_exceptions {
                    let json =
                        serde_json::to_string(exception).expect("Failed to serialize exception");
                    writeln!(file, "{}", json).expect("Failed to write exception");
                }
            }
        }
    }

    if let Some(taxdump_out) = options.out.clone() {
        let root_taxon_ids = options.root_taxon_id.clone();
        let leaf_taxon_ids = options
            .leaf_taxon_id
            .clone()
            .map(|ids| ids.into_iter().collect::<HashSet<_>>());
        let base_taxon_id = options.base_taxon_id.clone();
        let output_format = options
            .output_format
            .clone()
            .unwrap_or(vec![cli::TaxonomyFormat::NCBI]);
        nodes.write_taxdump(
            root_taxon_ids,
            leaf_taxon_ids,
            base_taxon_id,
            &taxdump_out,
            output_format,
            false,
        );
    }
    Ok(())
}
