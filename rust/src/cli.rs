use std::collections::HashSet;
use std::path::PathBuf;
use std::str::FromStr;
// use std::str::FromStr;
// use std::string::ParseError;

use clap::{ArgGroup, Parser, Subcommand, ValueEnum};
use clap_num::number_range;

#[cfg(feature = "python-extension")]
use pyo3::pyclass;

use rust_decimal::RoundingStrategy;
use serde;
use serde::{Deserialize, Serialize};

use crate::plot::axis::Scale;
use crate::plot::data::Reducer;
use crate::plot::ShowLegend;

// fn float_range(s: &str, min: f64, max: f64) -> Result<f64, String> {
//     debug_assert!(min <= max, "minimum of {} exceeds maximum of {}", min, max);
//     let val = match s.parse::<f64>() {
//         Ok(v) => v,
//         Err(e) => panic!("{:?}", e),
//     };
//     check_float_range(val, min, max)
// }

// fn check_float_range(val: f64, min: f64, max: f64) -> Result<f64, String> {
//     if val > max {
//         Err(format!("exceeds maximum of {}", max))
//     } else if val < min {
//         Err(format!("less than minimum of {}", min))
//     } else {
//         Ok(val)
//     }
// }

// fn window_size_range(s: &str) -> Result<f64, String> {
//     float_range(s, 0.0001, 1000000000.0)
// }

fn bin_size_parser(s: &str) -> Result<usize, String> {
    let mut val = match s.parse::<usize>() {
        Ok(v) => v,
        Err(e) => panic!("{:?}", e),
    };
    if val == 0 {
        val = usize::MAX
    }
    Ok(val)
}

/// Top level arguments to `blobtk`
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(subcommand_required = true, arg_required_else_help = true)]
pub struct Arguments {
    #[clap(subcommand)]
    pub cmd: SubCommand,
}

/// `blobtk` subcommands
#[derive(Subcommand, Debug)]
pub enum SubCommand {
    /// Calculate sequencing coverage depth.
    /// Called as `blobtk depth`
    Depth(DepthOptions),
    /// Filter files based on list of sequence names.
    /// Called as `blobtk filter`
    Filter(FilterOptions),
    /// [experimental] Index files for GenomeHubs.
    /// Called as `blobtk index`
    Index(IndexOptions),
    /// Process a BlobDir and produce static plots.
    /// Called as `blobtk plot`
    Plot(PlotOptions),
    /// Produce a snail plot from a BlobDir or FASTA file.
    /// Called as `blobtk snail`
    Snail(SnailOptions),
    /// [experimental] Process a taxonomy and lookup lineages, or start the API server with --api
    /// Called as `blobtk taxonomy [--api] ...`
    Taxonomy(TaxonomyOptions),
    /// [experimental] Validate BlobToolKit and GenomeHubs files.
    /// Called as `blobtk validate`
    Validate(ValidateOptions),
    /// Create a minimal BlobDir from input files (FASTA and optional BUSCO)
    /// Called as `blobtk create`
    Create(CreateOptions),
}

/// Options to pass to `blobtk depth`
#[derive(Parser, Debug)]
#[command(group(
    ArgGroup::new("alignment")
        .required(false)
        .args(["bam", "cram"]),
))]
#[command(arg_required_else_help = true)]
#[cfg_attr(feature = "python-extension", pyclass)]
pub struct DepthOptions {
    /// List of sequence IDs
    // Skipping this attribute because it is set to a default value using serde
    #[clap(skip)]
    pub list: Option<HashSet<Vec<u8>>>,
    /// Path to input file containing a list of sequence IDs
    #[arg(long = "list", short = 'i', value_name = "TXT")]
    pub list_file: Option<PathBuf>,
    /// Path to BAM file
    #[arg(long, short = 'b')]
    pub bam: Option<PathBuf>,
    /// Path to CRAM file
    #[arg(long, short = 'c')]
    pub cram: Option<PathBuf>,
    /// Path to assembly FASTA input file (required for CRAM)
    #[arg(long, short = 'a')]
    pub fasta: Option<PathBuf>,
    /// Bin size for coverage calculations (use 0 for full contig length)
    #[arg(long = "bin-size", short = 's', default_value_t = 0, value_parser = bin_size_parser)]
    pub bin_size: usize,
    // /// Window size for coverage calculations size
    // #[arg(long = "window-size", short = 'w', num_args(1..), default_values_t = [1.0], value_parser = window_size_range, action = clap::ArgAction::Append)]
    // pub window_size: Vec<f64>,
    /// Output bed file name
    #[arg(long = "bed", short = 'O', value_name = "BED")]
    pub bed: Option<PathBuf>,
}

/// Options to pass to `blobtk filter`
#[derive(Parser, Debug)]
#[command(group(
    ArgGroup::new("alignment")
        .required(false)
        .args(["bam", "cram"]),
))]
#[command(arg_required_else_help = true)]
#[cfg_attr(feature = "python-extension", pyclass)]
pub struct FilterOptions {
    // TODO: add option to invert list (use BAM header)
    /// List of sequence IDs
    #[clap(skip)]
    pub list: Option<HashSet<Vec<u8>>>,
    /// Path to input file containing a list of sequence IDs
    #[arg(long = "list", short = 'i', value_name = "TXT")]
    pub list_file: Option<PathBuf>,
    /// Path to BAM file
    #[arg(long, short = 'b')]
    pub bam: Option<PathBuf>,
    /// Path to CRAM file
    #[arg(long, short = 'c', requires = "fasta")]
    pub cram: Option<PathBuf>,
    /// Path to assembly FASTA input file (required for CRAM)
    #[arg(long, short = 'a')]
    pub fasta: Option<PathBuf>,
    /// Path to FASTQ file to filter (forward or single reads)
    #[arg(long = "fastq", short = 'f', value_name = "FASTQ")]
    pub fastq1: Option<PathBuf>,
    /// Path to paired FASTQ file to filter (reverse reads)
    #[arg(
        long = "fastq2",
        short = 'r',
        value_name = "FASTQ",
        requires = "fastq1"
    )]
    pub fastq2: Option<PathBuf>,
    /// Suffix to use for output filtered files
    #[arg(long, short = 'S', value_name = "SUFFIX", default_value_t = String::from("filtered"))]
    pub suffix: String,
    /// Flag to output a filtered FASTA file
    #[arg(
        long = "fasta-out",
        short = 'A',
        requires = "fasta",
        default_value_t = false
    )]
    pub fasta_out: bool,
    /// Flag to output filtered FASTQ files
    #[arg(
        long = "fastq-out",
        short = 'F',
        requires = "fastq1",
        default_value_t = false
    )]
    pub fastq_out: bool,
    /// Path to output list of read IDs
    #[arg(long = "read-list", short = 'O', value_name = "TXT")]
    pub read_list: Option<PathBuf>,
}

/// Options to pass to `blobtk index`
#[derive(Parser, Debug)]
#[command(arg_required_else_help = true)]
// #[cfg_attr(feature = "python-extension", pyclass)]
pub struct IndexOptions {
    /// Path to BlobDir directory containing files to index
    #[arg(long, short = 'b')]
    pub blobdir: Option<PathBuf>,
    /// Window sizes for feature extraction.
    /// Supported values depend on the BlobDir (typically 0.01, 0.1, 1, 100000, 1000000)
    #[arg(long = "window-size", short = 'w', num_args(1..), default_values_t = [1.0], value_parser = window_size_parser, action = clap::ArgAction::Append)]
    pub window_size: Vec<f64>,
    /// Taxon ID for the assembly
    #[arg(long = "taxon-id", short = 't')]
    pub taxon_id: Option<String>,
    /// Assembly accession to fetch sequence report from NCBI datasets
    #[arg(long = "accession", short = 'a')]
    pub datasets_accession: Option<String>,
    /// Path to BUSCO directory containing files to index
    /// Multiple BUSCO directories can be provided
    #[arg(long, short = 'u', num_args(1..), action = clap::ArgAction::Append)]
    pub busco: Option<Vec<PathBuf>>,
    /// Flag to generate JSON schema
    #[arg(long, short = 'g')]
    pub schema: bool,
    /// Output schema file name
    #[arg(long, short = 'O')]
    pub out: Option<PathBuf>,
}

/// Options to pass to `blobtk create`
#[derive(Parser, Debug, Default)]
#[cfg_attr(feature = "python-extension", pyclass)]
pub struct CreateOptions {
    /// Path to input assembly FASTA file
    #[arg(short = 'a', long = "fasta", value_name = "FASTA")]
    pub fasta: Option<PathBuf>,
    /// Path to BUSCO full_table.tsv or similar
    #[arg(short = 'b', long = "busco", value_name = "BUSCO")]
    pub busco: Option<PathBuf>,
    /// Output BlobDir directory
    #[arg(short = 'd', long = "out", value_name = "BLOB_DIR")]
    pub out: Option<PathBuf>,
}

fn window_size_parser(s: &str) -> Result<f64, String> {
    let val = match s.parse::<f64>() {
        Ok(v) => v,
        Err(e) => panic!("{:?}", e),
    };
    Ok(val)
}

#[derive(ValueEnum, Clone, Debug, Default)]
#[cfg_attr(feature = "python-extension", pyclass)]
pub enum View {
    #[default]
    Blob,
    Cumulative,
    Legend,
    Snail,
}

impl FromStr for View {
    type Err = ();
    fn from_str(input: &str) -> Result<View, Self::Err> {
        match input {
            "blob" => Ok(View::Blob),
            "cumulative" => Ok(View::Cumulative),
            "legend" => Ok(View::Legend),
            "snail" => Ok(View::Snail),
            _ => Ok(View::Blob),
        }
    }
}

#[derive(ValueEnum, Clone, Debug, Default)]
#[cfg_attr(feature = "python-extension", pyclass)]
pub enum Shape {
    #[default]
    Circle,
    Grid,
}

impl FromStr for Shape {
    type Err = ();
    fn from_str(input: &str) -> Result<Shape, Self::Err> {
        match input {
            "circle" => Ok(Shape::Circle),
            "grid" => Ok(Shape::Grid),
            _ => Ok(Shape::Circle),
        }
    }
}

#[derive(ValueEnum, Clone, Debug)]
#[cfg_attr(feature = "python-extension", pyclass)]
pub enum Origin {
    O,
    X,
    Y,
}

impl FromStr for Origin {
    type Err = ();
    fn from_str(input: &str) -> Result<Origin, Self::Err> {
        match input {
            "o" => Ok(Origin::O),
            "x" => Ok(Origin::X),
            "y" => Ok(Origin::Y),
            _ => Ok(Origin::O),
        }
    }
}

#[derive(ValueEnum, Clone, Debug)]
#[cfg_attr(feature = "python-extension", pyclass)]
pub enum Palette {
    Default,
    Inverse,
    Viridis,
}

impl FromStr for Palette {
    type Err = ();
    fn from_str(input: &str) -> Result<Palette, Self::Err> {
        match input {
            "default" => Ok(Palette::Default),
            "inverse" => Ok(Palette::Inverse),
            "viridis" => Ok(Palette::Viridis),
            _ => Err(()),
        }
    }
}

#[derive(ValueEnum, Clone, Debug)]
#[cfg_attr(feature = "python-extension", pyclass)]
pub enum ScoreType {
    #[value(name = "base")]
    Base, // unadjusted snail score
    #[value(name = "g")]
    G, // genome-size-adjusted
    #[value(name = "gs")]
    Gs, // genome + scaffold-adjusted
    #[value(name = "ag")]
    GAbsolute, // absolute (penalizes both directions)
    #[value(name = "ags")]
    GsAbsolute, // absolute with both corrections
}

impl FromStr for ScoreType {
    type Err = ();
    fn from_str(input: &str) -> Result<ScoreType, Self::Err> {
        match input {
            "base" => Ok(ScoreType::Base),
            "g" => Ok(ScoreType::G),
            "gs" => Ok(ScoreType::Gs),
            "ag" => Ok(ScoreType::GAbsolute),
            "ags" => Ok(ScoreType::GsAbsolute),
            _ => Err(()),
        }
    }
}

fn less_than_5(s: &str) -> Result<f64, String> {
    Ok(number_range(&format!("{}", s.parse::<f64>().unwrap() * 10.0), 2, 50)? as f64 / 10.0)
}

const BADGE_LINEAR_SCALE_ERROR: &str =
    "--badge requires --scale-function linear (badge mode only supports linear scaling)";

fn validate_badge_scale(badge: bool, scale_function: &Scale) -> Result<(), anyhow::Error> {
    if badge && scale_function != &Scale::LINEAR {
        return Err(anyhow::anyhow!(BADGE_LINEAR_SCALE_ERROR));
    }
    Ok(())
}

/// Options to pass to `blobtk plot`
#[derive(Clone, Parser, Debug, Default)]
#[command(arg_required_else_help = true)]
#[cfg_attr(feature = "python-extension", pyclass)]
pub struct PlotOptions {
    /// Path to BlobDir directory
    #[arg(long, short = 'd')]
    pub blobdir: PathBuf,
    /// View to plot
    #[arg(long, short = 'v')]
    #[clap(value_enum)]
    pub view: View,
    /// Plot shape for blob plot
    #[arg(long)]
    #[clap(value_enum)]
    pub shape: Option<Shape>,
    /// Window size for grid shape plot
    #[arg(long = "window-size", short = 'w')]
    pub window_size: Option<String>,
    /// Output filename(s) - can be specified multiple times
    /// Supports .svg, .png, .json, .yaml extensions
    #[arg(long, short = 'o', value_name = "FILE", action = clap::ArgAction::Append)]
    pub output: Vec<String>,
    /// Optional reference BlobDir for snail plot
    #[arg(long = "reference", value_name = "REFERENCE")]
    pub reference: Option<PathBuf>,
    /// Filters to apply to BlobDir data.
    ///
    /// Preferred syntax: <field>:<type>=<value>
    /// Legacy BlobToolKit URL-compatible syntax is also supported: <field>--<Type>=<value>
    ///
    /// Types:
    /// - min / max for numeric fields (inclusive bounds)
    /// - keys for categorical/string fields (comma-separated)
    /// - inv to invert a filter (either as <field>:inv or <field>:inv=<keys>)
    ///
    /// Examples:
    /// - --filter length:min=1000
    /// - --filter gc:max=0.6
    /// - --filter buscogenes_phylum:keys=Arthropoda,Chordata
    /// - --filter buscogenes_phylum:inv=Arthropoda,Chordata
    #[arg(
        long,
        short = 'f',
        value_name = "FILTER",
        help = "Filter expression(s), e.g. length:min=1000 or gc:max=0.6"
    )]
    pub filter: Vec<String>,
    /// Segment count for snail plot
    #[arg(long, short = 's', default_value_t = 1000)]
    pub segments: usize,
    /// Max span for snail plot
    #[arg(long = "max-span")]
    pub max_span: Option<usize>,
    /// max scaffold length for snail plot
    #[arg(long = "max-scaffold")]
    pub max_scaffold: Option<usize>,
    /// X-axis field for blob plot
    #[arg(long = "x-field", short = 'x')]
    pub x_field: Option<String>,
    /// Y-axis field for blob plot
    #[arg(long = "y-field", short = 'y')]
    pub y_field: Option<String>,
    /// Z-axis field for blob plot
    #[arg(long = "z-field", short = 'z')]
    pub z_field: Option<String>,
    /// Category field for blob plot
    #[arg(long = "category", short = 'c')]
    pub cat_field: Option<String>,
    /// Field to use for sequence identifier synonyms
    #[arg(long = "synonyms")]
    pub synonym_field: Option<String>,
    /// Resolution for blob plot
    #[arg(long, default_value_t = 30)]
    pub resolution: usize,
    /// Maximum histogram height for blob plot
    #[arg(long = "hist-height")]
    pub hist_height: Option<usize>,
    /// Reducer function for blob plot
    #[arg(long, value_enum, default_value_t = Reducer::Sum)]
    pub reducer_function: Reducer,
    /// Scale function for blob/snail plot
    ///
    /// Defaults to `sqrt` for blob view and `linear` for snail view.
    #[arg(long, value_enum)]
    pub scale_function: Option<Scale>,
    /// Scale factor for blob plot (0.2 - 5.0)
    #[arg(long, default_value_t = 1.0, value_parser=less_than_5)]
    pub scale_factor: f64,
    /// X-axis limits for blob/cumulative plot (<min>,<max>)
    #[arg(long = "x-limit")]
    pub x_limit: Option<String>,
    /// Y-axis limits for blob/cumulative plot (<min>,<max>)
    #[arg(long = "y-limit")]
    pub y_limit: Option<String>,
    /// Maximum number of categories for blob/cumulative plot
    #[arg(long = "cat-count", default_value_t = 10)]
    pub cat_count: usize,
    /// Maximum number of categories for blob/cumulative plot
    #[arg(long = "legend", value_enum, default_value_t = ShowLegend::Default)]
    pub show_legend: ShowLegend,
    /// Category order for blob/cumulative plot (<cat1>,<cat2>,...)
    #[arg(long = "cat-order")]
    pub cat_order: Option<String>,
    /// Origin for category lines in cumulative plot
    #[arg(long, value_enum)]
    pub origin: Option<Origin>,
    /// Colour palette for categories
    #[arg(long, value_enum)]
    pub palette: Option<Palette>,
    /// Individual colours to modify palette (<index>=<hexcode>)
    #[arg(long)]
    pub color: Option<Vec<String>>,
    /// Significant digits to use when rounding numbers for display
    #[arg(long = "significant-digits", default_value_t = 3)]
    pub significant_digits: u32,
    /// Decimal precision (number of decimal places) to use when percentages for display
    #[arg(long = "decimal-precision", default_value_t = 2)]
    pub decimal_precision: u32,
    // [experimental] Flag to choose the rounding method
    #[arg(long = "rounding", value_enum)]
    pub rounding: Option<RoundingStrategyWrapper>,
    /// Flag to show numbers instead of percentages in snail plot legend
    #[arg(long = "show-numbers", default_value_t = false)]
    pub show_numbers: bool,
    /// Flag to show busco numbers instead of percentages in snail plot legend
    #[arg(long = "busco-numbers", default_value_t = false)]
    pub busco_numbers: bool,
    /// Flag to show minimal snail plot as assembly badge
    #[arg(long = "badge", default_value_t = false)]
    pub badge: bool,
    /// Flag to show snail score in snail plot legend
    #[arg(long = "show-score", default_value_t = false)]
    pub show_score: bool,
    /// Score variant to display with --show-score
    ///
    /// - base: unadjusted snail score
    /// - g: genome-size-adjusted (requires --max-span or --reference)
    /// - gs: genome and scaffold-adjusted (requires --max-span/--reference and --max-scaffold)
    /// - ag: absolute adjustment (penalizes both over/underassembly)
    /// - ags: absolute with both corrections
    #[arg(long, value_name = "TYPE")]
    pub score_type: Option<ScoreType>,
    /// Internal field to track original FASTA input (for snail command)
    #[clap(skip)]
    pub original_fasta: Option<String>,
    /// Internal field to track original BUSCO input (for snail command)
    #[clap(skip)]
    pub original_busco: Option<String>,
    /// Internal field to track original BlobDir input (for snail command)
    #[clap(skip)]
    pub original_blobdir: Option<String>,
    /// Internal field to track original reference input (for snail command)
    #[clap(skip)]
    pub original_reference: Option<String>,
    /// Internal field to track score-only mode (for snail command)
    #[clap(skip)]
    pub score_only: bool,
    /// Internal field to track score JSON output mode (for snail command)
    #[clap(skip)]
    pub score_json: bool,
}

impl PlotOptions {
    /// Resolve scale function, applying view-specific defaults when unset.
    pub fn resolved_scale_function(&self) -> Scale {
        self.scale_function
            .clone()
            .unwrap_or_else(|| match self.view {
                View::Snail => Scale::LINEAR,
                _ => Scale::SQRT,
            })
    }

    /// Validate that score_type requirements are met
    pub fn validate_score_type(&self) -> Result<(), anyhow::Error> {
        validate_badge_scale(self.badge, &self.resolved_scale_function())?;

        match &self.score_type {
            Some(ScoreType::Base) | None => Ok(()),
            Some(ScoreType::G) | Some(ScoreType::GAbsolute) => {
                if self.max_span.is_some() || self.reference.is_some() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!(
                        "--score-type g or ag requires either --max-span or --reference"
                    ))
                }
            }
            Some(ScoreType::Gs) | Some(ScoreType::GsAbsolute) => {
                let has_max_span = self.max_span.is_some();
                let has_max_scaffold = self.max_scaffold.is_some();
                let has_ref = self.reference.is_some();

                if has_ref || (has_max_span && has_max_scaffold) {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!(
                        "--score-type gs or ags requires both --max-span and --max-scaffold, or a --reference"
                    ))
                }
            }
        }
    }
}

#[derive(ValueEnum, Clone, Debug, Default)]
#[cfg_attr(feature = "python-extension", pyclass)]
pub enum RoundingStrategyWrapper {
    #[default]
    #[clap(name = "round")]
    MidpointAwayFromZero,
    #[clap(name = "down")]
    ToZero,
    #[clap(name = "up")]
    AwayFromZero,
}

impl From<RoundingStrategyWrapper> for RoundingStrategy {
    fn from(wrapper: RoundingStrategyWrapper) -> Self {
        match wrapper {
            RoundingStrategyWrapper::MidpointAwayFromZero => RoundingStrategy::MidpointAwayFromZero,
            RoundingStrategyWrapper::ToZero => RoundingStrategy::ToZero,
            RoundingStrategyWrapper::AwayFromZero => RoundingStrategy::AwayFromZero,
        }
    }
}

impl From<RoundingStrategy> for RoundingStrategyWrapper {
    fn from(strategy: RoundingStrategy) -> Self {
        match strategy {
            RoundingStrategy::MidpointAwayFromZero => RoundingStrategyWrapper::MidpointAwayFromZero,
            RoundingStrategy::ToZero => RoundingStrategyWrapper::ToZero,
            RoundingStrategy::AwayFromZero => RoundingStrategyWrapper::AwayFromZero,
            _ => RoundingStrategyWrapper::MidpointAwayFromZero,
        }
    }
}

impl RoundingStrategyWrapper {
    pub fn from_str(input: &str) -> Result<RoundingStrategy, ()> {
        match input {
            "round" => Ok(RoundingStrategy::MidpointAwayFromZero),
            "down" => Ok(RoundingStrategy::ToZero),
            "up" => Ok(RoundingStrategy::AwayFromZero),
            _ => Err(()),
        }
    }

    pub fn default() -> RoundingStrategy {
        RoundingStrategy::MidpointAwayFromZero
    }
}

/// Options to pass to `blobtk plot`
#[derive(Parser, Debug, Default)]
#[command(arg_required_else_help = true)]
#[cfg_attr(feature = "python-extension", pyclass)]
pub struct SnailOptions {
    /// Path to input assembly FASTA file
    #[arg(long = "fasta", value_name = "FASTA")]
    pub fasta: Option<PathBuf>,
    /// Path to BUSCO full_table.tsv or similar
    #[arg(long = "busco", value_name = "BUSCO")]
    pub busco: Option<PathBuf>,
    /// Path to BlobDir directory
    #[arg(long, short = 'd')]
    pub blobdir: Option<PathBuf>,
    /// Reference assembly for snail plot (BlobDir or FASTA)
    #[arg(long = "reference", value_name = "REFERENCE")]
    pub reference: Option<PathBuf>,
    /// Output filename(s) - can be specified multiple times
    /// Supports .svg, .png, .json, .yaml extensions
    #[arg(long, short = 'o', value_name = "FILE", action = clap::ArgAction::Append)]
    pub output: Vec<String>,
    /// Assembly name for display in snail plot legend (defaults to filename or BlobDir name)
    #[arg(long = "assembly-name", short = 'n')]
    pub assembly_name: Option<String>,
    /// Reference assembly name for display in snail plot legend (defaults to filename or BlobDir name)
    #[arg(long = "reference-name", short = 'r')]
    pub reference_name: Option<String>,
    /// Report snail score to stdout (base type by default)
    #[arg(long)]
    pub score_only: bool,
    /// Report snail score as JSON to stdout
    #[arg(long)]
    pub score_json: bool,
    /// Filters to apply to scaffold data before snail stats are computed.
    ///
    /// Preferred syntax: <field>:<type>=<value>
    /// Legacy BlobToolKit URL-compatible syntax is also supported: <field>--<Type>=<value>
    ///
    /// Types:
    /// - min / max for numeric fields (inclusive bounds)
    /// - keys for categorical/string fields (comma-separated)
    /// - inv to invert a filter (either as <field>:inv or <field>:inv=<keys>)
    ///
    /// Examples:
    /// - --filter length:min=1000
    /// - --filter gc:max=0.6
    /// - --filter buscogenes_phylum:keys=Arthropoda,Chordata
    /// - --filter buscogenes_phylum:inv=Arthropoda,Chordata
    #[arg(
        long,
        short = 'f',
        value_name = "FILTER",
        help = "Filter expression(s), e.g. length:min=1000 or gc:max=0.6"
    )]
    pub filter: Vec<String>,
    /// Segment count for snail plot
    #[arg(long, short = 's', default_value_t = 1000)]
    pub segments: usize,
    /// Max span for snail plot
    #[arg(long = "max-span")]
    pub max_span: Option<usize>,
    /// max scaffold length for snail plot
    #[arg(long = "max-scaffold")]
    pub max_scaffold: Option<usize>,
    /// Scale function for snail plot
    #[arg(long, value_enum, default_value_t = Scale::LINEAR)]
    pub scale_function: Scale,
    /// Significant digits to use when rounding numbers for display
    #[arg(long = "significant-digits", default_value_t = 3)]
    pub significant_digits: u32,
    /// Decimal precision (number of decimal places) to use when percentages for display
    #[arg(long = "decimal-precision", default_value_t = 2)]
    pub decimal_precision: u32,
    // [experimental] Flag to choose the rounding method
    #[arg(long = "rounding", value_enum)]
    pub rounding: Option<RoundingStrategyWrapper>,
    /// Flag to show numbers instead of percentages in snail plot legend
    #[arg(long = "show-numbers", default_value_t = false)]
    pub show_numbers: bool,
    /// Flag to show busco numbers instead of percentages in snail plot legend
    #[arg(long = "busco-numbers", default_value_t = false)]
    pub busco_numbers: bool,
    /// Flag to show minimal snail plot as assembly badge
    #[arg(long = "badge", default_value_t = false)]
    pub badge: bool,
    /// Flag to show snail score in snail plot legend
    #[arg(long = "show-score", default_value_t = false)]
    pub show_score: bool,

    /// Score variant to display with --show-score
    ///
    /// - base: unadjusted snail score
    /// - g: genome-size-adjusted (requires --max-span or --reference)
    /// - gs: genome and scaffold-adjusted (requires --max-span/--reference and --max-scaffold)
    /// - ag: absolute adjustment (penalizes both over/underassembly)
    /// - ags: absolute with both corrections
    #[arg(long, value_name = "TYPE")]
    pub score_type: Option<ScoreType>,
}

impl SnailOptions {
    /// Validate that score_type requirements are met
    pub fn validate_score_type(&self) -> Result<(), anyhow::Error> {
        validate_badge_scale(self.badge, &self.scale_function)?;

        match &self.score_type {
            Some(ScoreType::Base) | None => Ok(()),
            Some(ScoreType::G) | Some(ScoreType::GAbsolute) => {
                if self.max_span.is_some() || self.reference.is_some() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!(
                        "--score-type g or ag requires either --max-span or --reference"
                    ))
                }
            }
            Some(ScoreType::Gs) | Some(ScoreType::GsAbsolute) => {
                let has_max_span = self.max_span.is_some();
                let has_max_scaffold = self.max_scaffold.is_some();
                let has_ref = self.reference.is_some();

                if has_ref || (has_max_span && has_max_scaffold) {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!(
                        "--score-type gs or ags requires both --max-span and --max-scaffold, or a --reference"
                    ))
                }
            }
        }
    }
}

/// Valid taxonomy formats
#[derive(ValueEnum, Parser, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaxonomyFormat {
    /// NCBI taxdump containing nodes.dmp, names.dmp and merged.dmp
    NCBI,
    /// GBIF simple format backbone taxonomy
    GBIF,
    /// ENA taxonomy record formatted as JSONL
    ENA,
    /// OTT (Open Tree of Life) taxonomy containing taxonomy.tsv, synonyms.tsv and forwards.tsv
    OTT,
    /// GenomeHubs TSV/YAML file pair
    GenomeHubs,
    /// GenomeHubs compatible JSONL file
    JSONL,
}

impl std::fmt::Display for TaxonomyFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TaxonomyFormat::NCBI => "ncbi",
            TaxonomyFormat::GBIF => "gbif",
            TaxonomyFormat::ENA => "ena",
            TaxonomyFormat::OTT => "ott",
            TaxonomyFormat::GenomeHubs => "genomehubs",
            TaxonomyFormat::JSONL => "jsonl",
        };
        write!(f, "{}", s)
    }
}

/// Options to pass to `blobtk taxonomy`
#[derive(Default, Parser, Serialize, Deserialize, Clone, Debug)]
#[command(arg_required_else_help = true)]
#[cfg_attr(feature = "python-extension", pyclass)]
pub struct TaxonomyOptions {
    /// Path to backbone taxonomy file/directory
    #[arg(long = "taxdump", short = 't')]
    pub path: Option<PathBuf>,
    /// Format of taxonomy file
    #[arg(long = "taxonomy-format", short = 'f')]
    pub taxonomy_format: Option<TaxonomyFormat>,
    /// Root taxon/taxa for filtered taxonomy
    #[arg(long = "root-id", short = 'r')]
    pub root_taxon_id: Option<Vec<String>>,
    /// Leaf taxon/taxa for filtered taxonomy
    #[arg(long = "leaf-id", short = 'l')]
    pub leaf_taxon_id: Option<Vec<String>>,
    /// Base taxon for filtered taxonomy lineages
    #[arg(long = "base-id", short = 'b')]
    pub base_taxon_id: Option<String>,
    /// Path to output filtered backbone taxonomy
    #[arg(long = "taxdump-out", short = 'O')]
    pub out: Option<PathBuf>,
    /// Output format for filtered backbone taxonomy
    #[arg(long = "output-format", short = 'F')]
    pub output_format: Option<Vec<TaxonomyFormat>>,
    /// Path to YAML format config file
    #[arg(long = "config", short = 'c')]
    pub config_file: Option<PathBuf>,
    /// List of name_classes to use during taxon lookup
    #[clap(skip)]
    #[serde(default = "default_name_classes")]
    pub name_classes: Vec<String>,
    /// Label to use when setting as xref
    #[arg(long = "xref-label", short = 'x')]
    pub xref_label: Option<String>,
    /// List of taxonomies to map to backbone
    #[clap(skip)]
    pub taxonomies: Option<Vec<TaxonomyOptions>>,
    /// Flag to create missing taxa if higher rank matches
    #[clap(skip)]
    #[serde(default = "default_create_taxa")]
    pub create_taxa: bool,
    /// Files to match to taxIDs - Experimental
    #[arg(long = "genomehubs_files", short = 'g')]
    pub genomehubs_files: Option<Vec<PathBuf>>,
    /// [experimental] Start the taxonomy API server instead of running a one-off merge/output
    #[arg(long = "api")]
    #[serde(default)]
    pub api: bool,
    /// Port to run the API server on (if --api is set)
    #[arg(long, short = 'p', default_value_t = 3000)]
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_name_classes() -> Vec<String> {
    vec!["scientific name".to_string()]
}

fn default_create_taxa() -> bool {
    false
}

fn default_port() -> u16 {
    3000
}

/// Options to pass to `blobtk validate`
#[derive(Default, Parser, Serialize, Deserialize, Clone, Debug)]
#[command(arg_required_else_help = true)]
#[cfg_attr(feature = "python-extension", pyclass)]
pub struct ValidateOptions {
    /// Path to backbone taxonomy file/directory
    #[arg(long = "taxdump", short = 't')]
    pub taxdump: Option<PathBuf>,
    /// Format of taxonomy file
    #[arg(long = "taxonomy-format", short = 'f')]
    pub taxonomy_format: Option<TaxonomyFormat>,
    /// List of name_classes to use during taxon lookup
    #[clap(long = "name-classes", short = 'n', default_value = "scientific name")]
    #[serde(default = "default_name_classes")]
    pub name_classes: Vec<String>,
    /// Files to match to taxIDs - Experimental
    #[arg(long = "genomehubs_files", short = 'g')]
    pub genomehubs_files: Option<Vec<PathBuf>>,
    /// Path to output JSON Schema file
    #[arg(long = "schema", short = 'S')]
    pub schema: Option<PathBuf>,
    // Dry run flag
    #[arg(long = "dry-run", short = 'd')]
    pub dry_run: bool,
    // Skip TSV flag
    #[arg(long = "skip-tsv", short = 'k')]
    pub skip_tsv: bool,
}

/// Command line argument parser
pub fn parse() -> Arguments {
    Arguments::parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plot_badge_rejects_non_linear_scale() {
        let options = PlotOptions {
            badge: true,
            scale_function: Some(Scale::SQRT),
            ..Default::default()
        };

        let err = options.validate_score_type().unwrap_err();
        assert_eq!(err.to_string(), BADGE_LINEAR_SCALE_ERROR);
    }

    #[test]
    fn plot_badge_accepts_snail_default_linear_scale() {
        let options = PlotOptions {
            badge: true,
            view: View::Snail,
            scale_function: None,
            ..Default::default()
        };

        assert!(options.validate_score_type().is_ok());
    }

    #[test]
    fn snail_badge_rejects_non_linear_scale() {
        let options = SnailOptions {
            badge: true,
            scale_function: Scale::SQRT,
            ..Default::default()
        };

        let err = options.validate_score_type().unwrap_err();
        assert_eq!(err.to_string(), BADGE_LINEAR_SCALE_ERROR);
    }
}
