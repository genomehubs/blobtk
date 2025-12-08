use anyhow;
use schemars::schema_for;

use crate::cli;
use crate::io::get_file_writer;
use crate::parse::genomehubs::GHubsConfig;
use crate::parse::lookup::build_fast_lookup;
use crate::parse::nodes::Nodes;
use crate::parse::parse_file;
use crate::taxonomy;

pub use cli::TaxonomyOptions;

pub use taxonomy::taxdump_to_nodes;

/// Execute the `validate` subcommand from `blobtk`.
pub fn validate(options: &cli::ValidateOptions) -> Result<(), anyhow::Error> {
    // let mut id_map = TreeMap::new();
    // id_map.insert(CString::new(clean_name("test")).unwrap(), 1);
    // let mut name = "Accipiter tachiro".to_string();
    // id_map.insert(CString::new(clean_name(&name)).unwrap(), 2);
    // // let res = id_map.get(&CString::new("test").unwrap());
    // let res = id_map.get(&CString::new(clean_name(&name)).unwrap());
    // dbg!(&name);
    // dbg!(clean_name(&name));
    // dbg!(res);

    // exit(1);
    let mut nodes = Nodes {
        ..Default::default()
    };
    if let Some(schema_file) = options.schema.clone() {
        let schema = schema_for!(GHubsConfig);
        let file_writer = get_file_writer(&schema_file, false);
        serde_json::to_writer_pretty(file_writer, &schema)?;
        return Ok(());
    }
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
            eprintln!("Parsing file: {:?}", genomehubs_file);
            let (_new_nodes, _new_names, _source) = parse_file(
                genomehubs_file,
                &id_map,
                !options.dry_run,
                false,
                None,
                options.skip_tsv,
            )?;
        }
    }
    Ok(())
}
