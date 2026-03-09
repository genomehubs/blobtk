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
    reference_blobdir: Option<std::path::PathBuf>,
) -> Result<cli::PlotOptions, anyhow::Error> {
    // check blobdir is not none before unwrapping
    if blobdir.is_none() {
        return Err(anyhow::anyhow!("BlobDir is required for snail plot"));
    }

    // Track what the user originally provided
    let (original_fasta, original_busco, original_blobdir) = if options.blobdir.is_some() {
        (
            None,
            None,
            Some(
                options
                    .blobdir
                    .as_ref()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
            ),
        )
    } else {
        (
            options
                .fasta
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            options
                .busco
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            None,
        )
    };

    Ok(cli::PlotOptions {
        output: options.output.clone(),
        blobdir: blobdir.unwrap(),
        view: cli::View::Snail,
        filter: options.filter.clone(),
        reference: reference_blobdir,
        segments: options.segments,
        max_span: options.max_span,
        max_scaffold: options.max_scaffold,
        scale_function: Some(options.scale_function.clone()),
        significant_digits: options.significant_digits,
        decimal_precision: options.decimal_precision,
        rounding: options.rounding.clone(),
        show_numbers: options.show_numbers,
        busco_numbers: options.busco_numbers,
        badge: options.badge.clone(),
        show_score: options.show_score,
        score_type: options.score_type.clone(),
        original_fasta,
        original_busco,
        original_blobdir,
        score_only: options.score_only,
        score_json: options.score_json,
        ..Default::default()
    })
}

/// Execute the `snail` subcommand from `blobtk`.
pub fn snail(options: &cli::SnailOptions) -> Result<(), anyhow::Error> {
    // If reference is provided, load snail plot data from the reference (BlobDir or FASTA)
    let mut reference_meta = None;
    let mut reference_blobdir = None;
    if let Some(reference) = options.reference.as_ref() {
        if let Ok(blobdir_meta) = blobdir::parse_blobdir(reference) {
            // If it's a valid BlobDir, use it directly
            reference_meta = Some(blobdir_meta);
            reference_blobdir = Some(reference.clone());
        } else {
            // If it's not a directory, assume it's a FASTA file and create a BlobDir from it
            reference_blobdir = Some(std::env::temp_dir().join("blobtk_snail_reference_blobdir"));
            create::create(&cli::CreateOptions {
                fasta: Some(reference.clone()),
                busco: None,
                out: reference_blobdir.clone(),
            })?;
            let blobdir_meta = blobdir::parse_blobdir(&reference_blobdir.clone().unwrap())?;
            reference_meta = Some(blobdir_meta);
        }
        if !reference.to_string_lossy().contains("://") && !reference.exists() {
            return Err(anyhow::anyhow!(
                "Reference file does not exist: {}",
                reference.display()
            ));
        }
    }

    // Parse the BlobDir metadata and data
    // if the blobdir does not exist, create a blobdir instead of erroring
    let mut plotted = false;
    if let Some(blobdir) = options.blobdir.as_ref() {
        if let Ok(blobdir_meta) = blobdir::parse_blobdir(blobdir) {
            plot_snail(
                &blobdir_meta,
                &reference_meta,
                &snail_opts_to_plot_opts(
                    options,
                    Some(blobdir.clone()),
                    reference_blobdir.clone(),
                )?,
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
            plot_snail(
                &blobdir_meta,
                &reference_meta,
                &snail_opts_to_plot_opts(options, blobdir, reference_blobdir.clone())?,
            )?;
        } else {
            return Err(anyhow::anyhow!(
                "No BlobDir found and no input FASTA file provided"
            ));
        }
    }
    Ok(())
}
