//!
//! Invoked by calling:
//! `blobtk plot <args>`

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow;
use num_integer::sqrt;

#[cfg(feature = "python-extension")]
use pyo3::pyclass;

use crate::blobdir;
use crate::blobdir::parse_field_identifiers;
use crate::blobdir::parse_field_synonym;
use crate::cli;
use crate::cli::Shape;
use crate::error;
use crate::plot::blob::BlobData;
use crate::plot::cumulative::CumulativeData;
// use crate::io;

use clap::ValueEnum;
pub use cli::PlotOptions;
use colorous;
use svg::Document;
use usvg::{fontdb, TreeParsing, TreeTextToPath};

use self::blob::BlobDimensions;
use self::chart::Dimensions;
use self::chart::TopRightBottomLeft;

/// Plot axis functions.
pub mod axis;

/// Blob plot functions.
pub mod blob;

/// Category functions.
pub mod category;

/// Chart options.
pub mod chart;

/// Chart components.
pub mod component;

/// Cumulative plot functions.
pub mod cumulative;

/// Scatter plot functions.
pub mod data;

/// Snail plot functions.
pub mod snail;

/// SVG styling functions.
pub mod style;

pub fn save_svg(document: &Document, options: &PlotOptions) {
    svg::save(options.output.as_str(), document).unwrap();
}

pub fn save_png(document: &Document, options: &PlotOptions) {
    let mut fontdb = fontdb::Database::new();
    fontdb.load_system_fonts();
    let mut buf = Vec::new();
    svg::write(&mut buf, document).unwrap();
    let opt = usvg::Options::default();
    let mut tree = usvg::Tree::from_data(&buf.as_slice(), &opt).unwrap();
    tree.convert_text(&fontdb);

    let width = 2000;
    let height = (width as f64 * tree.size.height() / tree.size.width()) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
    resvg::render(
        &tree,
        resvg::FitTo::Size(width, height),
        tiny_skia::Transform::default(),
        pixmap.as_mut(),
    )
    .unwrap();
    pixmap.save_png(options.output.as_str()).unwrap();
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Suffix {
    PNG,
    SVG,
    JSON,
    YAML,
}

impl FromStr for Suffix {
    type Err = ();
    fn from_str(input: &str) -> Result<Suffix, Self::Err> {
        match input {
            "png" => Ok(Suffix::PNG),
            "svg" => Ok(Suffix::SVG),
            "json" => Ok(Suffix::JSON),
            "yaml" => Ok(Suffix::YAML),
            _ => Err(()),
        }
    }
}

#[derive(ValueEnum, Clone, Debug, Default)]
#[cfg_attr(feature = "python-extension", pyclass)]
pub enum ShowLegend {
    #[default]
    Default,
    Full,
    Compact,
    None,
}

impl FromStr for ShowLegend {
    type Err = ();
    fn from_str(input: &str) -> Result<ShowLegend, Self::Err> {
        match input {
            "default" => Ok(ShowLegend::Default),
            "full" => Ok(ShowLegend::Full),
            "compact" => Ok(ShowLegend::Compact),
            "none" => Ok(ShowLegend::None),
            _ => Ok(ShowLegend::Default),
        }
    }
}

/// Make a snail plot
pub fn plot_snail(meta: &blobdir::Meta, options: &cli::PlotOptions) -> Result<(), anyhow::Error> {
    let gc_values = blobdir::parse_field_float("gc".to_string(), &options.blobdir)?;
    let length_values = blobdir::parse_field_int("length".to_string(), &options.blobdir)?;
    let n_values = blobdir::parse_field_float("n".to_string(), &options.blobdir);
    let ncount_values = blobdir::parse_field_int("ncount".to_string(), &options.blobdir)?;
    let id = meta.id.clone();
    let record_type = meta.record_type.clone();

    let filters = blobdir::parse_filters(&options, None);
    let wanted_indices = blobdir::set_filters(filters, &meta, &options.blobdir);

    let gc_filtered = blobdir::apply_filter_float(&gc_values, &wanted_indices);
    let n_filtered = match n_values {
        Ok(values) => Some(blobdir::apply_filter_float(&values, &wanted_indices)),
        Err(_) => None,
    };
    let length_filtered = blobdir::apply_filter_int(&length_values, &wanted_indices);
    let ncount_filtered = blobdir::apply_filter_int(&ncount_values, &wanted_indices);
    let busco_list = meta.busco_list.clone();
    let (busco_total, busco_lineage, busco_filtered) = match busco_list {
        Some(list) if !list.is_empty() => {
            let busco_field = list[0].clone();
            let busco_values = blobdir::parse_field_busco(busco_field.0, &options.blobdir).unwrap();
            let busco_total = busco_field.1;
            let busco_lineage = busco_field.2;
            let busco_filtered = blobdir::apply_filter_busco(&busco_values, &wanted_indices);
            (Some(busco_total), Some(busco_lineage), busco_filtered)
        }
        _ => (None, None, vec![]),
    };

    let snail_stats = snail::snail_stats(
        &length_filtered,
        &gc_filtered,
        &n_filtered,
        &ncount_filtered,
        &busco_filtered,
        busco_total,
        busco_lineage,
        id,
        record_type,
        &options,
    );
    let suffix = get_suffix(options)?;
    match suffix {
        Suffix::JSON => {
            serde_json::to_writer_pretty(std::fs::File::create(&options.output)?, &snail_stats)?;
            return Ok(());
        }
        Suffix::YAML => {
            serde_yaml::to_writer(std::fs::File::create(&options.output)?, &snail_stats)?;
            return Ok(());
        }
        _ => {
            // For SVG/PNG, continue to create the document and save
            let document: Document = snail::svg(&snail_stats, &options);
            save_by_suffix(options, document)?;
            Ok(())
        }
    }
}

fn get_suffix(options: &PlotOptions) -> Result<Suffix, error::Error> {
    let output_str = options.output.as_str();
    let suffix_str = PathBuf::from(output_str)
        .extension()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    Suffix::from_str(&suffix_str).map_err(|_| error::Error::InvalidImageSuffix(suffix_str))
}

fn save_by_suffix(options: &PlotOptions, document: Document) -> Result<(), error::Error> {
    let suffix = get_suffix(options)?;

    match suffix {
        Suffix::PNG => save_png(&document, &options),
        Suffix::SVG => save_svg(&document, &options),
        _ => return Err(error::Error::InvalidImageSuffix(format!("{:?}", suffix))),
    };
    Ok(())
}

/// Convert a colorous::Color to 6 digit hex string
/// # Examples
///
/// ```
/// # use colorous::Color;
/// # use crate::blobtk::plot::color_to_hex;
/// assert_eq!(color_to_hex(Color {r: 255, g: 127, b: 0}), "#ff7f00");
pub fn color_to_hex(color: colorous::Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

pub fn reverse_palette(count: usize) -> Vec<String> {
    let gradient = colorous::PAIRED;
    let mut list = vec![];
    for i in 0..count {
        let mut j = if i % 2 == 1 { i - 1 } else { i + 1 };
        j = j % 12;
        list.push(color_to_hex(gradient[j]));
    }
    list
}

pub fn default_palette(count: usize) -> Vec<String> {
    let gradient = colorous::PAIRED;
    let mut list = vec![];
    for i in 0..count {
        let j = i % 12;
        list.push(color_to_hex(gradient[j]));
    }
    list
}

pub fn set_palette(
    name: &Option<cli::Palette>,
    colors: &Option<Vec<String>>,
    count: usize,
) -> Vec<String> {
    let mut color_list = match name {
        Some(cli::Palette::Default) | None => default_palette(count),
        Some(cli::Palette::Inverse) => reverse_palette(count),
        Some(cli::Palette::Viridis) => {
            let gradient = colorous::VIRIDIS;
            (0..count)
                .map(|i| color_to_hex(gradient.eval_rational(i, count)))
                .collect()
        }
    };
    if colors.is_some() {
        for color in colors.clone().unwrap() {
            let (index, hex) = color.split_once("=").unwrap();
            let i: usize = index.parse().unwrap();
            let mut hexcode = hex.to_string();
            if i <= count {
                hexcode = hexcode.replace("hex", "#");
                if !hexcode.starts_with("#") {
                    hexcode = format!("#{}", hexcode);
                }
                color_list[i] = hexcode;
            }
        }
    }
    color_list
}

fn insert_hashmap_option(
    hash: &mut HashMap<String, String>,
    tag: String,
    primary: Option<String>,
    secondary: Option<String>,
    tertiary: Option<String>,
) -> Result<(), error::Error> {
    if primary.is_some() {
        hash.insert(tag, primary.unwrap());
    } else if secondary.is_some() {
        hash.insert(tag, secondary.unwrap());
    } else if tertiary.is_some() {
        hash.insert(tag, tertiary.unwrap());
    } else {
        return Err(error::Error::AxisNotDefined(tag));
    }
    Ok(())
}

fn set_blob_filters(
    options: &PlotOptions,
    meta: &blobdir::Meta,
) -> Result<
    (
        HashMap<String, String>,
        HashMap<String, Vec<f64>>,
        Vec<usize>,
        Vec<f64>,
        Vec<category::Category>,
        Vec<Option<usize>>,
    ),
    anyhow::Error,
> {
    let mut plot_meta: HashMap<String, String> = HashMap::new();
    insert_hashmap_option(
        &mut plot_meta,
        "x".to_string(),
        options.x_field.clone(),
        meta.plot.x.clone(),
        None,
    )?;
    insert_hashmap_option(
        &mut plot_meta,
        "y".to_string(),
        options.y_field.clone(),
        meta.plot.y.clone(),
        None,
    )?;
    insert_hashmap_option(
        &mut plot_meta,
        "z".to_string(),
        options.z_field.clone(),
        meta.plot.z.clone(),
        None,
    )?;
    insert_hashmap_option(
        &mut plot_meta,
        "cat".to_string(),
        options.cat_field.clone(),
        meta.plot.cat.clone(),
        Some("_".to_string()),
    )?;
    let (plot_values, cat_values) = blobdir::get_plot_values(&meta, &options.blobdir, &plot_meta)?;
    let palette = set_palette(&options.palette, &options.color, options.cat_count);
    let (cat_order, cat_indices) = category::set_cat_order(
        &cat_values,
        &plot_values["z"],
        &options.cat_order,
        &options.cat_count,
        &palette,
    );
    let filters = blobdir::parse_filters(&options, Some(&plot_meta));
    let wanted_indices = blobdir::set_filters(filters, &meta, &options.blobdir);
    let z = blobdir::apply_filter_float(&plot_values["z"], &wanted_indices);
    let filtered_cat_values = blobdir::apply_filter_cat_tuple(&cat_values, &wanted_indices);
    let (cat_order, cat_indices) = if wanted_indices.len() < plot_values["x"].len() {
        category::set_cat_order(
            &filtered_cat_values,
            &z,
            &Some(
                cat_order[1..]
                    .iter()
                    .map(|x| x.members.join(","))
                    .collect::<Vec<String>>()
                    .join(","),
            ),
            &options.cat_count,
            &palette,
        )
    } else {
        (cat_order, cat_indices)
    };
    Ok((
        plot_meta,
        plot_values,
        wanted_indices,
        z,
        cat_order,
        cat_indices,
    ))
}

fn set_blob_data(
    options: &PlotOptions,
    meta: &blobdir::Meta,
) -> Result<(HashMap<String, String>, BlobData), anyhow::Error> {
    let (plot_meta, plot_values, wanted_indices, z, cat_order, cat_indices) =
        set_blob_filters(options, meta)?;

    let blob_data = BlobData {
        x: blobdir::apply_filter_float(&plot_values["x"], &wanted_indices),
        y: blobdir::apply_filter_float(&plot_values["y"], &wanted_indices),
        z,
        cat: cat_indices,
        cat_order,
        title: Some(meta.id.clone()),
    };
    Ok((plot_meta, blob_data))
}

pub fn plot_blob(meta: &blobdir::Meta, options: &cli::PlotOptions) -> Result<(), anyhow::Error> {
    let (plot_meta, blob_data) = set_blob_data(options, meta)?;

    let dimensions = BlobDimensions {
        ..Default::default()
    };

    let scatter_data = blob::blob_points(plot_meta, &blob_data, &dimensions, &meta, &options, None);

    let (x_bins, y_bins, max_bin) =
        blob::bin_axes(&scatter_data, &blob_data, &dimensions, &options);

    let document: Document = blob::plot(
        dimensions,
        scatter_data,
        x_bins,
        y_bins,
        max_bin,
        max_bin,
        &options,
    );
    save_by_suffix(options, document)?;
    Ok(())
}

fn set_grid_data(
    options: &PlotOptions,
    meta: &blobdir::Meta,
) -> Result<
    (
        HashMap<String, String>,
        Vec<BlobData>,
        HashMap<String, [f64; 2]>,
    ),
    anyhow::Error,
> {
    let (plot_meta, _plot_values, wanted_indices, _z, cat_order, _cat_indices) =
        set_blob_filters(options, meta)?;
    let (window_values, window_cat_values, limits) = blobdir::get_window_values(
        &meta,
        &options.blobdir,
        &plot_meta,
        &wanted_indices,
        &options.window_size,
    )?;
    let identifiers = parse_field_identifiers("identifiers".to_string(), &options.blobdir)?;
    let filtered_identifiers;
    if let Some(synonym_field) = &options.synonym_field {
        let synonyms = parse_field_synonym(synonym_field.clone(), &options.blobdir)?;
        filtered_identifiers = blobdir::apply_filter_option_string_with_fallback(
            &synonyms,
            &wanted_indices,
            &identifiers,
        );
    } else {
        filtered_identifiers = blobdir::apply_filter_string(&identifiers, &wanted_indices);
    }
    let mut grid_data = vec![];
    for (i, x) in window_values["x"].iter().enumerate() {
        if x.is_empty() {
            continue;
        }
        grid_data.push(BlobData {
            x: x.clone(),
            y: window_values["y"][i].clone(),
            z: window_values["z"][i].clone(),
            cat: if let Some(cat_values) = window_cat_values.get(i) {
                cat_values
                    .iter()
                    .map(|c| match c {
                        Some((_, idx)) => Some(idx.to_owned() + 1),
                        None => None,
                    })
                    .collect()
            } else {
                vec![]
            },
            cat_order: cat_order.clone(),
            title: Some(filtered_identifiers[i].clone()),
        })
    }
    Ok((plot_meta, grid_data, limits))
}

#[derive(Clone, Debug)]
pub struct GridSize {
    num_items: usize,
    height: f64,
    width: f64,
    num_rows: usize,
    // num_cols: usize,
    row_height: f64,
    // col_width: f64,
    col_widths: Vec<f64>,
    ratios: Vec<f64>,
    margin: TopRightBottomLeft,
    padding: TopRightBottomLeft,
    outer_margin: TopRightBottomLeft,
}

impl Default for GridSize {
    fn default() -> GridSize {
        let dimensions = Dimensions {
            ..Default::default()
        };
        GridSize {
            num_items: 1,
            height: dimensions.height,
            width: dimensions.width,
            num_rows: 1,
            // num_cols: 1,
            row_height: dimensions.height,
            // col_width: dimensions.width,
            col_widths: vec![dimensions.width],
            ratios: vec![1.0],
            margin: TopRightBottomLeft {
                ..Default::default()
            },
            padding: TopRightBottomLeft {
                ..Default::default()
            },
            outer_margin: TopRightBottomLeft {
                ..Default::default()
            },
        }
    }
}

impl GridSize {
    pub fn new(num_items: usize, dimensions: &BlobDimensions, ratios: Option<Vec<f64>>) -> Self {
        let (num_cols, num_rows) = calculate_grid_size(num_items);
        let height = dimensions.height;
        let width = dimensions.width;
        let bottom_left_margin = 25.0;
        let top_right_margin = 0.0;
        let padding = 10.0;
        let outer_bottom_left_margin = 50.0;
        let outer_top_right_margin = dimensions.margin.right;
        let col_width = match ratios {
            Some(_) => {
                (width
                    - outer_bottom_left_margin
                    - outer_top_right_margin
                    - (padding * 2.0 + bottom_left_margin + top_right_margin) * num_cols as f64)
                    / num_cols as f64
            }
            None => (width - outer_bottom_left_margin - outer_top_right_margin) / num_cols as f64,
        };
        let col_widths = match ratios.as_ref() {
            Some(r) => {
                let ratio_sum = r.iter().sum::<f64>();
                r.iter()
                    .map(|x| {
                        (x * col_width / ratio_sum * num_cols as f64)
                            + padding * 2.0
                            + bottom_left_margin
                            + top_right_margin
                    })
                    .collect()
            }
            None => vec![col_width; num_cols],
        };
        GridSize {
            num_items,
            height,
            width,
            num_rows,
            // num_cols,
            row_height: (height - outer_bottom_left_margin - outer_top_right_margin)
                / num_rows as f64,
            // col_width,
            col_widths,
            ratios: match ratios {
                Some(ref r) => r.clone(),
                None => vec![1.0; num_cols],
            },
            margin: TopRightBottomLeft {
                top: 10.0,
                right: top_right_margin,
                bottom: bottom_left_margin,
                left: bottom_left_margin,
            },
            padding: TopRightBottomLeft {
                top: padding,
                right: padding,
                bottom: padding,
                left: padding,
            },
            outer_margin: TopRightBottomLeft {
                top: outer_top_right_margin,
                right: outer_top_right_margin,
                bottom: outer_bottom_left_margin,
                left: outer_bottom_left_margin,
            },
        }
    }
}
/// Calculates the minimum and maximum dimensions for a grid layout based on the
/// number of items. Returns the dimensions as a [min, max] tuple.
fn calculate_grid_size(num_items: usize) -> (usize, usize) {
    // return early if count is 0
    let mut grid_size = [0; 2];
    // Grid should be as close to square as possible
    if num_items > 0 {
        grid_size = [sqrt(num_items); 2];
    }
    // if not a perfect square, add one to the max dimension
    if num_items > grid_size[0] * grid_size[1] {
        grid_size[1] += 1;
    }
    // if not an n(n+1) rectangle, add one to the max dimension
    if num_items > grid_size[0] * grid_size[1] {
        grid_size[1] += 1;
    }
    // if not an n(n+2) rectangle, make an (n+1)(n+1) larger square
    if num_items > grid_size[0] * grid_size[1] {
        grid_size[0] += 1;
        grid_size[1] -= 1;
    }
    (grid_size[0], grid_size[1])
}

pub fn plot_grid(meta: &blobdir::Meta, options: &cli::PlotOptions) -> Result<(), anyhow::Error> {
    let (plot_meta, grid_data, limits) = set_grid_data(options, meta)?;

    let dimensions = BlobDimensions {
        ..Default::default()
    };
    let mut ratios = None;
    if Some("position".to_string()) == options.x_field
        && options.x_limit.is_none()
        && grid_data.len() > 0
    {
        let (_, num_rows) = calculate_grid_size(grid_data.len());
        let max_values = grid_data
            .chunks(num_rows)
            .map(|chunk| {
                chunk
                    .iter()
                    .map(|x| x.x.iter().max_by(|a, b| a.partial_cmp(b).unwrap()))
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
            })
            .collect::<Vec<_>>();
        let x_max = limits["x"][1];
        ratios = Some(
            max_values
                .iter()
                .map(|x| (x.unwrap().unwrap() / x_max * 10.0).ceil() / 10.0)
                .collect::<Vec<_>>(),
        );
    }
    let grid_size = GridSize::new(grid_data.len(), &dimensions, ratios);
    let mut scatter_data = vec![];
    let mut titles = vec![];
    let mut col = 0;
    for (i, blob_data) in grid_data.iter().enumerate() {
        titles.push(blob_data.title.clone());
        scatter_data.push(blob::blob_points(
            plot_meta.clone(),
            &blob_data,
            &BlobDimensions {
                height: grid_size.row_height
                    - grid_size.padding.top
                    - grid_size.padding.bottom
                    - grid_size.margin.top
                    - grid_size.margin.bottom,
                width: grid_size.col_widths[col]
                    - grid_size.padding.left
                    - grid_size.padding.right
                    - grid_size.margin.left
                    - grid_size.margin.right,
                padding: grid_size.padding.clone(),
                margin: grid_size.margin.clone(),
                ..Default::default()
            },
            &meta,
            &options,
            Some({
                let mut new_limits = limits.clone();
                new_limits.get_mut("x").unwrap()[1] *= grid_size.ratios[col];
                new_limits
            }),
        ));
        if (i + 1) % grid_size.num_rows == 0 {
            col += 1;
        }
    }
    // let blob_data = grid_data;

    // let mut updated_options = options.clone().to_owned();
    // updated_options.x_limit = Some(format!("{},{}", limits["x"][0], limits["x"][1]));
    let document: Document = blob::plot_grid(
        grid_size,
        scatter_data,
        titles,
        (plot_meta["x"].clone(), plot_meta["y"].clone()),
        &options,
    );
    save_by_suffix(options, document)?;
    Ok(())
}

pub fn plot_legend(meta: &blobdir::Meta, options: &cli::PlotOptions) -> Result<(), anyhow::Error> {
    let (plot_meta, blob_data) = set_blob_data(options, meta)?;

    let dimensions = BlobDimensions {
        ..Default::default()
    };

    let scatter_data = blob::blob_points(plot_meta, &blob_data, &dimensions, &meta, &options, None);

    let document: Document = blob::legend(dimensions, scatter_data, &options);
    save_by_suffix(options, document)?;
    Ok(())
}

pub fn plot_cumulative(
    meta: &blobdir::Meta,
    options: &cli::PlotOptions,
) -> Result<(), anyhow::Error> {
    let mut plot_meta: HashMap<String, String> = HashMap::new();
    plot_meta.insert("z".to_string(), "length".to_string());

    insert_hashmap_option(
        &mut plot_meta,
        "cat".to_string(),
        options.cat_field.clone(),
        meta.plot.cat.clone(),
        Some("_".to_string()),
    )?;
    let (plot_values, cat_values) = blobdir::get_plot_values(&meta, &options.blobdir, &plot_meta)?;

    let palette = set_palette(&options.palette, &options.color, options.cat_count);

    let (cat_order, cat_indices) = category::set_cat_order(
        &cat_values,
        &plot_values["z"],
        &options.cat_order,
        &options.cat_count,
        &palette,
    );
    // let id = meta.id.clone();
    // let record_type = meta.record_type.clone();

    let filters = blobdir::parse_filters(&options, None);
    let wanted_indices = blobdir::set_filters(filters, &meta, &options.blobdir);

    let cumulative_data = CumulativeData {
        values: blobdir::apply_filter_float(&plot_values["z"], &wanted_indices),
        cat: blobdir::apply_filter_option_int(&cat_indices, &wanted_indices),
        cat_order,
    };

    let dimensions = Dimensions {
        ..Default::default()
    };

    let cumulative_lines = cumulative::cumulative_lines(&cumulative_data, &dimensions, &options);

    let document: Document = cumulative::plot(dimensions, cumulative_lines, &options);
    save_by_suffix(options, document)?;
    Ok(())
}

/// Execute the `plot` subcommand from `blobtk`.
pub fn plot(options: &cli::PlotOptions) -> Result<(), anyhow::Error> {
    let meta = blobdir::parse_blobdir(&options.blobdir)?;
    let view = &options.view;
    let shape = &options.shape;
    match view {
        cli::View::Blob => match shape {
            Some(Shape::Grid) => plot_grid(&meta, &options)?,
            _ => plot_blob(&meta, &options)?,
        },
        cli::View::Cumulative => plot_cumulative(&meta, &options)?,
        cli::View::Legend => plot_legend(&meta, &options)?,
        cli::View::Snail => plot_snail(&meta, &options)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_grid_size_23() {
        let count = 23;
        let expected = (5, 5);
        let result = calculate_grid_size(count);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_calculate_grid_size_16() {
        let count = 16;
        let expected = (4, 4);
        let result = calculate_grid_size(count);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_calculate_grid_size_5() {
        let count = 5;
        let expected = (2, 3);
        let result = calculate_grid_size(count);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_calculate_grid_size_37() {
        let count = 37;
        let expected = (6, 7);
        let result = calculate_grid_size(count);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_calculate_grid_size_2() {
        let count = 2;
        let expected = (1, 2);
        let result = calculate_grid_size(count);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_calculate_grid_size_1() {
        let count = 1;
        let expected = (1, 1);
        let result = calculate_grid_size(count);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_calculate_grid_size_0() {
        let count = 1;
        let expected = (1, 1);
        let result = calculate_grid_size(count);
        assert_eq!(result, expected);
    }
}
