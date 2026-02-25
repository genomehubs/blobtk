//!
//! Invoked by calling:
//! `blobtk snail <args>`

use anyhow;

use crate::blobdir;
use crate::cli;
use crate::create;
use crate::plot::plot_snail;
// use crate::io;
pub use cli::PlotOptions;

fn snail_opts_to_plot_opts(
    options: &cli::SnailOptions,
    blobdir: Option<std::path::PathBuf>,
) -> Result<cli::PlotOptions, anyhow::Error> {
    // check blobdir is not none before unwrapping
    if blobdir.is_none() {
        return Err(anyhow::anyhow!("BlobDir is required for snail plot"));
    }

    Ok(cli::PlotOptions {
        output: options.output.clone(),
        blobdir: blobdir.unwrap(),
        view: cli::View::Snail,
        filter: options.filter.clone(),
        segments: options.segments,
        max_span: options.max_span,
        max_scaffold: options.max_scaffold,
        scale_function: options.scale_function.clone(),
        significant_digits: options.significant_digits,
        decimal_precision: options.decimal_precision,
        rounding: options.rounding.clone(),
        show_numbers: options.show_numbers,
        busco_numbers: options.busco_numbers,
        badge: options.badge.clone(),
        show_score: options.show_score,
        ..Default::default()
    })
}

/// Execute the `snail` subcommand from `blobtk`.
pub fn snail(options: &cli::SnailOptions) -> Result<(), anyhow::Error> {
    // Parse the BlobDir metadata and data
    // if the blobdir does not exist, create a blobdir instead of erroring
    let mut plotted = false;
    if let Some(blobdir) = options.blobdir.as_ref() {
        if blobdir.exists() {
            let blobdir_meta = blobdir::parse_blobdir(blobdir)?;
            plot_snail(
                &blobdir_meta,
                &snail_opts_to_plot_opts(options, Some(blobdir.clone()))?,
            )?;
            plotted = true;
        }
    }

    if !plotted {
        if let Some(fasta) = &options.fasta {
            if !fasta.to_string_lossy().contains("://") && !fasta.exists() {
                return Err(anyhow::anyhow!(
                    "Input FASTA file does not exist: {}",
                    fasta.display()
                ));
            }
            let mut blobdir = options.blobdir.clone();
            if blobdir.is_none() {
                blobdir = Some(std::env::temp_dir().join("blobtk_snail_blobdir"));
            }

            create::create(&cli::CreateOptions {
                fasta: Some(fasta.clone()),
                busco: options.busco.clone(),
                out: blobdir.clone(),
            })?;
            let blobdir_meta = blobdir::parse_blobdir(&blobdir.clone().unwrap())?;
            plot_snail(&blobdir_meta, &snail_opts_to_plot_opts(options, blobdir)?)?;
        } else {
            return Err(anyhow::anyhow!(
                "No BlobDir found and no input FASTA file provided"
            ));
        }
    }
    Ok(())
}
