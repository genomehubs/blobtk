use std::process;

use anyhow;

use blobtk::cli;
use blobtk::depth;
use blobtk::filter;
use blobtk::index;
use blobtk::plot;
use blobtk::taxonomy;
use blobtk::validate;

fn cmd(args: cli::Arguments) -> Result<(), anyhow::Error> {
    match args.cmd {
        cli::SubCommand::Depth(options) => depth::depth(&options)?,
        cli::SubCommand::Filter(options) => filter::filter(&options)?,
        cli::SubCommand::Index(options) => index::index(&options)?,
        cli::SubCommand::Plot(options) => plot::plot(&options)?,
        cli::SubCommand::Taxonomy(options) => taxonomy::taxonomy(&options)?,
        cli::SubCommand::Validate(options) => validate::validate(&options)?,
    }
    Ok(())
}
fn main() {
    let args = cli::parse();
    if let Err(e) = cmd(args) {
        eprintln!("ERROR: {e}");
        process::exit(1);
    }
}
