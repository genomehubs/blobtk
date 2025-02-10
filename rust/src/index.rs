//!
//! Invoked by calling:
//! `blobtk index <args>`

use anyhow;
use schemars::schema_for;
use serde_json::to_string_pretty;

use crate::cli;
use crate::io::get_writer;
use crate::parse::genomehubs::GHubsConfig;

pub use cli::IndexOptions;

/// Execute the `index` subcommand from `blobtk`.
pub fn index(options: &cli::IndexOptions) -> Result<(), anyhow::Error> {
    if options.schema {
        dbg!("testing");
        let schema = schema_for!(GHubsConfig);
        let mut writer = get_writer(&options.out);

        writeln!(&mut writer, "{}", to_string_pretty(&schema).unwrap())?;
    }
    Ok(())
}
