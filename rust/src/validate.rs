use anyhow;

use crate::cli;
use crate::parse::lookup::build_fast_lookup;
use crate::parse::nodes::Nodes;
use crate::parse::parse_file;
use crate::taxonomy;

pub use cli::TaxonomyOptions;

pub use taxonomy::taxdump_to_nodes;

/// Execute the `validate` subcommand from `blobtk`.
pub fn validate(options: &cli::ValidateOptions) -> Result<(), anyhow::Error> {
    let mut nodes = Nodes {
        ..Default::default()
    };
    if options.taxdump.is_some() {
        let taxonomy_options = TaxonomyOptions {
            path: options.taxdump.clone(),
            taxonomy_format: options.taxonomy_format.clone(),
            name_classes: options.name_classes.clone(),
            ..Default::default()
        };
        nodes = taxdump_to_nodes(&taxonomy_options, None)?;
    }
    nodes = nodes.clone();

    if let Some(genomehubs_files) = options.genomehubs_files.clone() {
        let id_map = build_fast_lookup(&nodes, &options.name_classes);
        for genomehubs_file in genomehubs_files {
            // match taxa to nodes
            // todo: add support for multiple genomehubs files
            println!("Parsing file: {:?}", genomehubs_file);
            let (_new_nodes, _new_names, _source) = parse_file(genomehubs_file, &id_map, true)?;
        }
    }
    Ok(())
}
