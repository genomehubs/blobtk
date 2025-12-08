use std::borrow::Borrow;
use std::collections::HashMap;

use std::str::FromStr;

use svg::node::element::{Group, Rectangle, Text};
use svg::node::Text as nodeText;
use svg::Document;

use crate::blobdir::FieldMeta;
use crate::cli::Shape;
use crate::utils::{max_float, min_float, scale_floats};
use crate::{blobdir, cli, plot};

use plot::category::Category;

use super::axis::{AxisName, AxisOptions, ChartAxes, Position, Scale, TickOptions};
use super::chart::{Chart, Dimensions, TopRightBottomLeft};
use super::component::{font_family, legend_group, LegendEntry, LegendShape};
use super::data::{Bin, HistogramData, Reducer, ScatterData, ScatterPoint};
use super::{GridSize, ShowLegend};

#[derive(Clone, Debug)]
pub struct BlobData {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub z: Vec<f64>,
    pub cat: Vec<Option<usize>>,
    pub cat_order: Vec<Category>,
    pub title: Option<String>,
}

#[derive(Clone, Debug)]
pub struct BlobDimensions {
    pub height: f64,
    pub width: f64,
    pub margin: TopRightBottomLeft,
    pub padding: TopRightBottomLeft,
    pub hist_height: f64,
    pub hist_width: f64,
}

impl Default for BlobDimensions {
    fn default() -> BlobDimensions {
        let dimensions = Dimensions {
            ..Default::default()
        };
        BlobDimensions {
            height: dimensions.height,
            width: dimensions.width,
            margin: dimensions.margin,
            padding: dimensions.padding,
            hist_height: 250.0,
            hist_width: 250.0,
        }
    }
}

fn scale_values(data: &Vec<f64>, meta: &AxisOptions) -> Vec<f64> {
    let mut scaled = vec![];
    for value in data {
        scaled.push(scale_floats(
            *value,
            &meta.domain,
            &meta.range,
            &meta.scale,
            meta.clamp.clone(),
        ));
    }
    scaled
}

pub fn bin_axis(
    scatter_data: &ScatterData,
    blob_data: &BlobData,
    axis: AxisName,
    options: &cli::PlotOptions,
) -> (Vec<Vec<f64>>, f64) {
    let range = match axis {
        AxisName::X => scatter_data.x.range.clone(),
        AxisName::Y => scatter_data.y.range.clone(),
        AxisName::Z => scatter_data.z.range.clone(),
        _ => [0.0, 100.0],
    };
    let bin_size = (range[1] - range[0]) / options.resolution as f64;
    let mut binned = vec![vec![0.0; options.resolution]; options.cat_count];
    let mut counts = vec![vec![0.0; options.resolution]; options.cat_count];
    let mut max_bin = 0.0;
    for point in scatter_data.points.iter() {
        let cat_index = point.cat_index;
        let mut bin = match axis {
            AxisName::X => ((point.x - range[0]) / bin_size).floor() as usize,
            AxisName::Y => ((point.y - range[0]) / bin_size).floor() as usize,
            AxisName::Z => ((point.z - range[0]) / bin_size).floor() as usize,
            _ => 0,
        };
        if bin == options.resolution {
            bin -= 1;
        }
        match options.reducer_function {
            Reducer::Sum => binned[cat_index][bin] += blob_data.z[point.data_index],
            Reducer::Max => {
                binned[cat_index][bin] =
                    max_float(binned[cat_index][bin], blob_data.z[point.data_index])
            }
            Reducer::Min => {
                binned[cat_index][bin] = if binned[cat_index][bin] == 0.0 {
                    blob_data.z[point.data_index]
                } else {
                    min_float(binned[cat_index][bin], blob_data.z[point.data_index])
                }
            }
            Reducer::Count => binned[cat_index][bin] += 1.0,
            Reducer::Mean => {
                binned[cat_index][bin] += blob_data.z[point.data_index];
                counts[cat_index][bin] += 1.0
            }
        };
        max_bin = max_float(max_bin, binned[cat_index][bin]);
    }
    match options.reducer_function {
        Reducer::Mean => {
            max_bin = 0.0;
            for (cat_index, bins) in binned.clone().iter().enumerate() {
                for (bin, _) in bins.iter().enumerate() {
                    if counts[cat_index][bin] > 0.0 {
                        binned[cat_index][bin] /= counts[cat_index][bin];
                        max_bin = max_float(max_bin, binned[cat_index][bin]);
                    }
                }
            }
        }
        Reducer::Min => {
            max_bin = 0.0;
            for (cat_index, bins) in binned.clone().iter().enumerate() {
                for (bin, _) in bins.iter().enumerate() {
                    max_bin = max_float(max_bin, binned[cat_index][bin]);
                }
            }
        }
        _ => (),
    }
    (binned, max_bin)
}

pub fn axis_hist(
    binned: Vec<Vec<f64>>,
    blob_data: &BlobData,
    dimensions: &BlobDimensions,
    max_bin: f64,
    axis: AxisName,
    options: &cli::PlotOptions,
) -> Vec<HistogramData> {
    let domain = [0.0, max_bin];
    let (width, range) = match axis {
        AxisName::X => (dimensions.width, [0.0, dimensions.hist_height]),
        _ => (dimensions.height, [0.0, dimensions.hist_width]),
    };
    let cat_order = blob_data.cat_order.clone();
    let bin_width = width / options.resolution as f64;
    let mut histograms = vec![
        HistogramData {
            max_bin,
            width,
            ..Default::default()
        };
        cat_order.len() - 1
    ];
    for (j, cat) in cat_order.iter().enumerate() {
        if j == 0 {
            continue;
        }
        let i = j - 1;
        histograms[i] = HistogramData {
            bins: binned[i]
                .iter()
                .map(|value| Bin {
                    height: scale_floats(*value, &domain, &range, &Scale::LINEAR, None),
                    width: bin_width,
                    value: *value,
                })
                .collect(),
            max_bin: scale_floats(max_bin, &domain, &range, &Scale::LINEAR, None),
            axis: axis.clone(),
            category: Some(cat.clone()),
            ..histograms[i]
        }
    }
    histograms
}

pub fn bin_axes(
    scatter_data: &ScatterData,
    blob_data: &BlobData,
    dimensions: &BlobDimensions,
    options: &cli::PlotOptions,
) -> (Vec<HistogramData>, Vec<HistogramData>, f64) {
    let (x_binned, x_max) = bin_axis(scatter_data, blob_data, AxisName::X, options);
    let (y_binned, y_max) = bin_axis(scatter_data, blob_data, AxisName::Y, options);
    let mut max_bin = max_float(x_max, y_max);
    if options.hist_height.is_some() {
        max_bin = max_float(max_bin, options.hist_height.unwrap() as f64)
    }
    let x_histograms = axis_hist(
        x_binned,
        blob_data,
        dimensions,
        max_bin,
        AxisName::X,
        options,
    );
    let y_histograms = axis_hist(
        y_binned,
        blob_data,
        dimensions,
        max_bin,
        AxisName::Y,
        options,
    );
    (x_histograms, y_histograms, max_bin)
}

fn set_domain(
    field_meta: &FieldMeta,
    limit_string: Option<String>,
    limit_arr: Option<[f64; 2]>,
    limit_clamp: Option<f64>,
) -> ([f64; 2], Option<f64>) {
    let clamp = match limit_clamp {
        Some(value) => value,
        None => 0.1,
    };
    let mut domain = field_meta.range.unwrap();
    if limit_string.is_some() {
        if let Some((min_value, max_value)) = limit_string.clone().unwrap().split_once(",") {
            if !min_value.is_empty() {
                domain[0] = min_value.parse::<f64>().unwrap();
            }
            if !max_value.is_empty() {
                domain[1] = max_value.parse::<f64>().unwrap();
            }
        }
    } else if limit_arr.is_some() {
        domain = limit_arr.clone().unwrap();
    }
    let clamp_value = if field_meta.clamp.is_some() {
        domain[0] = field_meta.range.unwrap()[0];
        field_meta.clamp
    } else if field_meta.range.unwrap()[0] == 0.0
        && field_meta.scale.clone().unwrap() == "scaleLog".to_string()
    {
        domain[0] = clamp;
        Some(clamp)
    } else {
        None
    };
    if domain[0] == domain[1] {
        if domain[0] == 0.0 {
            domain[1] += 0.1;
        } else {
            domain[0] /= 0.1;
            domain[1] *= 0.1;
        }
    }
    // if domain[0] == 0.005 {
    //     domain = [0.0, 1.0];
    // }
    (domain, clamp_value)
}

pub fn blob_points(
    axes: HashMap<String, String>,
    blob_data: &BlobData,
    dimensions: &BlobDimensions,
    meta: &blobdir::Meta,
    options: &cli::PlotOptions,
    limits: Option<HashMap<String, [f64; 2]>>,
) -> ScatterData {
    let fields = meta.field_list.clone().unwrap();
    let x_meta = fields[axes["x"].as_str()].clone();
    let (x_limit_arr, y_limit_arr) = match limits {
        Some(limit) => (Some(limit["x"]), Some(limit["y"])),
        None => (None, None),
    };
    let (x_domain, x_clamp) = set_domain(&x_meta, options.x_limit.clone(), x_limit_arr, None);
    let x_axis = AxisOptions {
        position: Position::BOTTOM,
        height: dimensions.height + dimensions.padding.top + dimensions.padding.bottom,
        label: axes["x"].clone(),
        padding: [dimensions.padding.left, dimensions.padding.right],
        offset: dimensions.height + dimensions.padding.top + dimensions.padding.bottom,
        scale: Scale::from_str(&x_meta.scale.unwrap()).unwrap(),
        domain: x_domain,
        range: [0.0, dimensions.width],
        clamp: x_clamp,
        ..Default::default()
    };
    let x_scaled = scale_values(&blob_data.x, &x_axis);

    let y_meta = fields[axes["y"].as_str()].clone();
    let (y_domain, y_clamp) = set_domain(&y_meta, options.y_limit.clone(), y_limit_arr, None);

    // if y_domain[0] == y_domain[1] {
    //     if y_domain[0] == 0.0 {
    //         y_domain[1] += 0.1;
    //     } else {
    //         y_domain[0] /= 2.0;
    //         y_domain[1] *= 2.0;
    //     }
    // }
    let y_axis = AxisOptions {
        position: Position::LEFT,
        height: dimensions.width + dimensions.padding.right + dimensions.padding.left,
        label: axes["y"].clone(),
        padding: [dimensions.padding.bottom, dimensions.padding.top],
        scale: Scale::from_str(&y_meta.scale.unwrap()).unwrap(),
        domain: y_domain,
        range: [dimensions.height, 0.0],
        clamp: y_clamp,
        rotate: true,
        ..Default::default()
    };
    let y_scaled = scale_values(&blob_data.y, &y_axis);

    let z_meta = fields[axes["z"].as_str()].clone();
    let mut z_domain = z_meta.range.unwrap().clone();
    if z_domain[0] == z_domain[1] {
        if z_domain[0] == 0.0 {
            z_domain[1] += 0.1;
        } else {
            z_domain[0] /= 2.0;
            z_domain[1] *= 2.0;
        }
    }
    let z_axis = AxisOptions {
        label: axes["z"].clone(),
        scale: options.scale_function.clone(),
        domain: z_domain,
        range: [2.0, 2.0 + dimensions.height / 15.0 * options.scale_factor],
        ..Default::default()
    };
    let z_scaled = scale_values(&blob_data.z, &z_axis);
    let mut points = vec![];
    let mut fg_points = vec![];
    match options.shape {
        Some(Shape::Grid) => {
            for (i, x) in x_scaled.iter().enumerate() {
                if blob_data.cat.len() <= i || blob_data.cat[i].is_none() {
                    points.push(ScatterPoint {
                        x: *x,
                        y: y_scaled[i],
                        z: z_scaled[i] * 1.5,
                        data_index: i,
                        ..Default::default()
                    });
                    continue;
                }
                if let Some(cat_index) = blob_data.cat[i] {
                    if blob_data.cat_order.len() < cat_index + 1 {
                        continue;
                    }
                    let cat = blob_data.cat_order[cat_index].clone();
                    let point = ScatterPoint {
                        x: *x,
                        y: y_scaled[i],
                        z: z_scaled[i] * 1.5,
                        label: Some(cat.title.clone()),
                        color: Some(cat.color.clone()),
                        cat_index,
                        data_index: i,
                    };
                    points.push(point.clone());
                    fg_points.push(point.clone());
                }
            }
            points.extend(fg_points);
        }
        _ => {
            let cat_order = blob_data.cat_order.clone();
            let mut ordered_points = vec![vec![]; cat_order.len() - 1];
            // TODO: add option to keep points together
            for (i, cat_index) in blob_data.cat.iter().enumerate() {
                if let Some(idx) = cat_index {
                    let cat = cat_order[*idx].borrow();
                    ordered_points[*idx - 1].push(ScatterPoint {
                        x: x_scaled[i],
                        y: y_scaled[i],
                        z: z_scaled[i],
                        label: Some(cat.title.clone()),
                        color: Some(cat.color.clone()),
                        cat_index: *idx - 1,
                        data_index: i,
                    })
                }
            }
            for cat_points in ordered_points {
                points.extend(cat_points);
            }
        }
    }
    ScatterData {
        points,
        x: x_axis,
        y: y_axis,
        z: z_axis,
        categories: blob_data.cat_order.clone(),
    }
}

pub fn category_legend_full(categories: Vec<Category>, show_legend: ShowLegend) -> Group {
    let mut entries = vec![];
    let title = "".to_string();
    match show_legend {
        ShowLegend::Full => entries.push(LegendEntry {
            subtitle: Some("[count; span; n50]".to_string()),
            shape: LegendShape::None,
            ..Default::default()
        }),
        _ => (),
    };
    for (i, cat) in categories.iter().enumerate() {
        if i == 0 {
            match show_legend {
                ShowLegend::Full => (),
                _ => continue,
            };
        }
        let subtitle = match show_legend {
            ShowLegend::Compact => None,
            ShowLegend::Default | ShowLegend::Full => Some(cat.clone().subtitle()),
            ShowLegend::None => return legend_group(title, entries, None, 1),
        };
        entries.push(LegendEntry {
            title: format!("{}", cat.title),
            color: cat.color.clone(),
            subtitle,
            ..Default::default()
        });
    }
    legend_group(title, entries, None, 1)
}

pub fn plot(
    blob_dimensions: BlobDimensions,
    scatter_data: ScatterData,
    hist_data_x: Vec<HistogramData>,
    hist_data_y: Vec<HistogramData>,
    x_max: f64,
    y_max: f64,
    options: &cli::PlotOptions,
) -> Document {
    let height = blob_dimensions.height
        + blob_dimensions.hist_height
        + blob_dimensions.margin.top
        + blob_dimensions.margin.bottom
        + blob_dimensions.padding.top
        + blob_dimensions.padding.bottom;

    let width = blob_dimensions.width
        + blob_dimensions.hist_width
        + blob_dimensions.margin.right
        + blob_dimensions.margin.left
        + blob_dimensions.padding.right
        + blob_dimensions.padding.left;
    let x_opts = scatter_data.x.clone();
    let y_opts = scatter_data.y.clone();

    let scatter = Chart {
        axes: ChartAxes {
            x: Some(x_opts.clone()),
            y: Some(y_opts.clone()),
            ..Default::default()
        },
        scatter_data: Some(scatter_data.clone()),
        dimensions: Dimensions {
            height: blob_dimensions.height,
            width: blob_dimensions.width,
            margin: blob_dimensions.margin,
            padding: blob_dimensions.padding,
        },
        ..Default::default()
    };

    let x_hist = Chart {
        axes: ChartAxes {
            x: Some(AxisOptions {
                label: "".to_string(),
                offset: blob_dimensions.hist_height,
                height: blob_dimensions.hist_height,
                tick_labels: false,
                ..x_opts.clone()
            }),
            y: Some(AxisOptions {
                position: Position::LEFT,
                label: "sum length".to_string(),
                label_offset: 80.0,
                height: blob_dimensions.width
                    + blob_dimensions.padding.right
                    + blob_dimensions.padding.left,
                font_size: 25.0,
                scale: Scale::LINEAR,
                domain: [0.0, x_max],
                range: [blob_dimensions.hist_height, 0.0],
                rotate: true,
                tick_count: 5,
                ..Default::default()
            }),
            x2: Some(AxisOptions {
                offset: 0.0,
                position: Position::TOP,
                major_ticks: None,
                minor_ticks: None,
                ..x_opts.clone()
            }),
            y2: Some(AxisOptions {
                position: Position::RIGHT,
                offset: blob_dimensions.width
                    + blob_dimensions.padding.right
                    + blob_dimensions.padding.left,
                scale: Scale::LINEAR,
                domain: [0.0, x_max],
                range: [blob_dimensions.hist_height, 0.0],
                major_ticks: None,
                minor_ticks: None,
                ..Default::default()
            }),
            ..Default::default()
        },
        histogram_data: Some(hist_data_x),
        dimensions: Dimensions {
            height: blob_dimensions.hist_height,
            width: blob_dimensions.width,
            margin: TopRightBottomLeft {
                ..Default::default()
            },
            padding: TopRightBottomLeft {
                right: blob_dimensions.padding.right,
                left: blob_dimensions.padding.left,
                ..Default::default()
            },
        },
        ..Default::default()
    };

    let y_hist = Chart {
        axes: ChartAxes {
            x: Some(AxisOptions {
                offset: 0.0,
                height: blob_dimensions.hist_height,
                label: "".to_string(),
                tick_labels: false,
                ..y_opts.clone()
            }),
            y: Some(AxisOptions {
                position: Position::BOTTOM,
                height: blob_dimensions.height
                    + blob_dimensions.padding.top
                    + blob_dimensions.padding.bottom,
                offset: blob_dimensions.height
                    + blob_dimensions.padding.top
                    + blob_dimensions.padding.bottom,
                label: "sum length".to_string(),
                label_offset: 80.0,
                font_size: 25.0,
                scale: Scale::LINEAR,
                domain: [0.0, y_max],
                range: [0.0, blob_dimensions.hist_width],
                tick_count: 5,
                rotate: true,
                ..Default::default()
            }),
            x2: Some(AxisOptions {
                offset: blob_dimensions.hist_width,
                position: Position::RIGHT,
                major_ticks: None,
                minor_ticks: None,
                label: "".to_string(),
                ..y_opts.clone()
            }),
            y2: Some(AxisOptions {
                position: Position::TOP,
                offset: 0.0,
                scale: Scale::LINEAR,
                domain: [0.0, y_max],
                range: [0.0, blob_dimensions.hist_width],
                major_ticks: None,
                minor_ticks: None,
                label: "".to_string(),
                ..Default::default()
            }),

            ..Default::default()
        },
        histogram_data: Some(hist_data_y),
        dimensions: Dimensions {
            height: blob_dimensions.hist_width,
            width: blob_dimensions.height,
            margin: TopRightBottomLeft {
                ..Default::default()
            },
            padding: TopRightBottomLeft {
                top: blob_dimensions.padding.top,
                bottom: blob_dimensions.padding.bottom,
                ..Default::default()
            },
        },
        ..Default::default()
    };

    let legend_x = match options.show_legend {
        ShowLegend::Compact => width - blob_dimensions.hist_width,
        _ => width - 185.0,
    };

    let opacity = 0.6;

    let document = Document::new()
        .set("viewBox", (0, 0, width, height))
        .add(
            Rectangle::new()
                .set("fill", "#ffffff")
                .set("stroke", "none")
                .set("width", width)
                .set("height", height),
        )
        .add(scatter.svg(0.0, 0.0, Some(opacity)).set(
            "transform",
            format!(
                "translate({}, {})",
                blob_dimensions.margin.left,
                blob_dimensions.hist_height + blob_dimensions.margin.top
            ),
        ))
        .add(x_hist.svg(0.0, 0.0, Some(opacity)).set(
            "transform",
            format!(
                "translate({}, {})",
                blob_dimensions.margin.left, blob_dimensions.margin.top
            ),
        ))
        .add(y_hist.svg(0.0, 0.0, Some(opacity)).set(
            "transform",
            format!(
                "translate({}, {})",
                blob_dimensions.margin.left
                    + blob_dimensions.width
                    + blob_dimensions.padding.right
                    + blob_dimensions.padding.left,
                blob_dimensions.hist_height + blob_dimensions.margin.top
            ),
        ))
        .add(
            category_legend_full(scatter_data.categories, options.show_legend.clone())
                .set("transform", format!("translate({}, {})", legend_x, 10.0)),
        );

    document
}

pub fn plot_grid(
    grid_size: GridSize,
    scatter_data: Vec<ScatterData>,
    titles: Vec<Option<String>>,
    labels: (String, String),
    _options: &cli::PlotOptions,
) -> Document {
    let x_label = labels.0;
    let y_label = labels.1;
    let height = grid_size.row_height - grid_size.margin.top - grid_size.margin.bottom;

    let mut charts = vec![];

    let offset = grid_size.row_height - grid_size.margin.bottom; // - grid_size.margin.bottom;
                                                                 // let y_range = [
                                                                 //     grid_size.row_height - grid_size.margin.top - grid_size.margin.bottom,
                                                                 //     grid_size.margin.bottom,
                                                                 // ];
    let y_range = [
        grid_size.row_height
            - grid_size.padding.bottom
            - grid_size.padding.top
            - grid_size.margin.bottom,
        grid_size.margin.top,
    ];

    let font_size = 20.0;
    // let line_weight = 2.0;

    for (i, data) in scatter_data.iter().enumerate() {
        let col = i / grid_size.num_rows;
        let row = i % grid_size.num_rows;
        let x_opts = data.x.clone();
        let y_opts = data.y.clone();
        let width = grid_size.col_widths[col] - grid_size.margin.left - grid_size.margin.right;

        let range = [
            grid_size.margin.left,
            grid_size.col_widths[col]
                - grid_size.padding.right
                - grid_size.padding.left
                - grid_size.margin.right,
        ];

        charts.push(Chart {
            axes: ChartAxes {
                x: Some(AxisOptions {
                    position: Position::BOTTOM,
                    height,
                    padding: [grid_size.padding.left, grid_size.padding.right],
                    offset,
                    scale: x_opts.scale.clone(),
                    domain: x_opts.domain.clone(),
                    range,
                    clamp: x_opts.clamp.clone(),
                    font_size,
                    weight: 1.0,
                    tick_count: 3,
                    tick_labels: row == grid_size.num_rows - 1 || i == grid_size.num_items - 1,
                    major_ticks: Some(TickOptions {
                        font_size: font_size * 0.75,
                        weight: 1.0,
                        length: 8.0,
                        ..Default::default()
                    }),
                    minor_ticks: Some(TickOptions {
                        font_size: font_size * 0.75,
                        weight: 1.0,
                        length: 5.0,
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                y: Some(AxisOptions {
                    position: Position::LEFT,
                    height: width,
                    offset: grid_size.margin.left,
                    padding: [grid_size.padding.top, grid_size.padding.bottom],
                    scale: y_opts.scale.clone(),
                    domain: y_opts.domain.clone(),
                    range: y_range,
                    clamp: y_opts.clamp.clone(),
                    font_size,
                    weight: 1.0,
                    tick_count: 3,
                    tick_labels: col == 0,
                    major_ticks: Some(TickOptions {
                        font_size: font_size * 0.75,
                        weight: 1.0,
                        length: 8.0,
                        ..Default::default()
                    }),
                    minor_ticks: Some(TickOptions {
                        font_size: font_size * 0.75,
                        weight: 1.0,
                        length: 5.0,
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            scatter_data: Some(data.clone()),
            dimensions: Dimensions {
                height: grid_size.row_height,
                width: grid_size.col_widths[col],
                margin: grid_size.margin,
                padding: grid_size.padding,
            },
            ..Default::default()
        });
    }

    // let legend_x = match options.show_legend {
    //     ShowLegend::Compact => grid_size.width - 100.0,
    //     _ => grid_size.width - 185.0,
    // };

    let processed_font_family = font_family("Roboto, Open sans, DejaVu Sans, Arial, sans-serif");

    let mut document = Document::new()
        .set("viewBox", (0, 0, grid_size.width, grid_size.height))
        .add(
            Rectangle::new()
                .set("fill", "#ffffff")
                .set("stroke", "none")
                .set("width", grid_size.width)
                .set("height", grid_size.height),
        )
        .add(
            Text::new()
                .set("font-family", processed_font_family.clone())
                .set("font-size", font_size * 1.25)
                .set("text-anchor", "middle")
                .set("dominant-baseline", "middle")
                .set("stroke", "none")
                // .set("fill", options.font_color.clone())
                .set(
                    "transform",
                    format!(
                        "translate({:?}, {:?}) rotate({:?})",
                        grid_size.outer_margin.left / 2.0,
                        (grid_size.height
                            - grid_size.outer_margin.bottom
                            - grid_size.outer_margin.top)
                            / 2.0
                            + grid_size.outer_margin.top,
                        90
                    ),
                )
                .add(nodeText::new(y_label)),
        )
        .add(
            Text::new()
                .set("font-family", processed_font_family.clone())
                .set("font-size", font_size * 1.25)
                .set("text-anchor", "middle")
                .set("dominant-baseline", "middle")
                .set("stroke", "none")
                // .set("fill", options.font_color.clone())
                .set(
                    "transform",
                    format!(
                        "translate({:?}, {:?})",
                        grid_size.outer_margin.left
                            + (grid_size.width
                                - grid_size.outer_margin.left
                                - grid_size.outer_margin.right)
                                / 2.0,
                        grid_size.height - grid_size.outer_margin.bottom / 2.0
                    ),
                )
                .add(nodeText::new(x_label)),
        );
    let mut i = 0;
    let mut x_offset = grid_size.outer_margin.left;
    for chart in charts {
        let col = i / grid_size.num_rows;
        let row = i % grid_size.num_rows;
        if row == 0 && col > 0 {
            x_offset += grid_size.col_widths[col - 1];
        }
        let y_offset = row as f64 * grid_size.row_height + grid_size.outer_margin.top;
        let mut group =
            Group::new().add(chart.svg(grid_size.margin.left, grid_size.margin.top, None));
        if let Some(ref title) = titles[i] {
            let mut title = title.clone();
            if title.len() > 3 && title.len() as f64 * 10.0 > grid_size.col_widths[col] {
                title = format!(
                    "{}...",
                    title[..(grid_size.col_widths[col] / 10.0) as usize - 3].to_string()
                );
            }
            group = group
                .add(
                    Text::new()
                        .set("font-family", processed_font_family.clone())
                        .set("font-size", font_size * 0.75)
                        .set("text-anchor", "middle")
                        .set("dominant-baseline", "hanging")
                        .set("stroke", "none")
                        // .set("fill", options.font_color.clone())
                        .set(
                            "transform",
                            format!(
                                "translate({:?}, {:?})",
                                grid_size.margin.left
                                    + (grid_size.col_widths[col]
                                        - grid_size.margin.left
                                        - grid_size.margin.right)
                                        / 2.0,
                                0.0
                            ),
                        )
                        .add(nodeText::new(title)),
                )
                .set(
                    "transform",
                    format!("translate({}, {})", x_offset, y_offset),
                );
        }
        document = document.add(group);
        i += 1;
    }

    document
}

pub fn legend(
    blob_dimensions: BlobDimensions,
    scatter_data: ScatterData,
    options: &cli::PlotOptions,
) -> Document {
    let height = scatter_data.categories.len() * 26;

    let mut width =
        blob_dimensions.hist_width + blob_dimensions.margin.left + blob_dimensions.padding.left;

    width = match options.show_legend {
        ShowLegend::Compact => width,
        _ => width + 220.0,
    };

    let offset_x = match options.show_legend {
        ShowLegend::Compact => 0.0,
        _ => width - 180.0,
    };

    let document = Document::new()
        .set("viewBox", (0, 0, width, height))
        .add(
            Rectangle::new()
                .set("fill", "#ffffff")
                .set("stroke", "none")
                .set("width", width)
                .set("height", height),
        )
        .add(
            category_legend_full(scatter_data.categories, options.show_legend.clone())
                .set("transform", format!("translate({}, {})", offset_x, 10.0)),
        );

    document
}
