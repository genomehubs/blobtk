use svg::node::element::Rectangle;
use svg::Document;

use crate::cli::Origin;
use crate::utils::linear_scale_float;
use crate::{cli, plot};

use plot::category::Category;

use super::axis::{AxisOptions, ChartAxes, Position, Scale};
use super::blob::category_legend_full;
use super::chart::{Chart, Dimensions};
use super::data::{Line, LineData};
use super::ShowLegend;

#[derive(Clone, Debug)]
pub struct CumulativeData {
    pub values: Vec<f64>,
    pub cat: Vec<usize>,
    pub cat_order: Vec<Category>,
}

pub fn cumulative_lines(
    cumulative_data: &CumulativeData,
    dimensions: &Dimensions,
    options: &cli::PlotOptions,
) -> LineData {
    let x_domain = [0.0, cumulative_data.values.len() as f64];
    let x_range = [0.0, dimensions.width];
    let x_axis = AxisOptions {
        position: Position::BOTTOM,
        label: "cumulative count".to_string(),
        height: dimensions.height + dimensions.padding.top + dimensions.padding.bottom,
        padding: [dimensions.padding.left, dimensions.padding.right],
        offset: dimensions.height + dimensions.padding.top + dimensions.padding.bottom,
        scale: Scale::LINEAR,
        domain: x_domain,
        range: x_range,
        ..Default::default()
    };
    let y_domain = [0.0, cumulative_data.values.iter().sum::<f64>()];
    let y_range = [dimensions.height, 0.0];
    let y_axis = AxisOptions {
        position: Position::LEFT,
        label_offset: 83.0,
        label: "cumulative length".to_string(),
        height: dimensions.width + dimensions.padding.right + dimensions.padding.left,
        padding: [dimensions.padding.bottom, dimensions.padding.top],
        scale: Scale::LINEAR,
        domain: y_domain,
        range: y_range,
        rotate: true,
        ..Default::default()
    };
    let mut lines = vec![];
    let mut cat_order = cumulative_data.cat_order.clone();
    match options.origin {
        Some(Origin::Y) => cat_order.sort_by(|x, y| y.span.cmp(&x.span)),
        _ => (),
    };
    // let mut ordered_points = vec![vec![]; cat_order.len()];
    let mut end_coords = [0.0, y_range[0]];
    for (index, cat) in cat_order.iter().enumerate() {
        let mut lengths = vec![];
        for i in cat.indices.iter() {
            lengths.push(cumulative_data.values[*i]);
        }
        lengths.sort_by(|a, b| b.partial_cmp(a).unwrap());
        let mut coords = vec![end_coords.clone()];
        let mut cumulative_span = 0.0;
        for (i, length) in lengths.iter().enumerate() {
            // add coords to line
            cumulative_span += length;
            coords.push([
                coords[0][0] + linear_scale_float((i + 1) as f64, &x_domain, &x_range),
                coords[0][1] - dimensions.height
                    + linear_scale_float(cumulative_span as f64, &y_domain, &y_range),
            ]);
        }
        if index > 0 {
            end_coords = match options.origin {
                Some(Origin::X) | Some(Origin::Y) => {
                    [coords[coords.len() - 1][0], coords[coords.len() - 1][1]]
                }
                _ => end_coords,
            };
        }
        lines.push(Line {
            coords,
            label: Some(cat.title.clone()),
            color: Some(cat.color.clone()),
            weight: 3.0,
            cat_index: index,
            ..Default::default()
        });

        //     ordered_points[*cat_index].push(ScatterPoint {
        //         x: x_scaled[i],
        //         y: y_scaled[i],
        //         z: z_scaled[i],
        //         label: Some(cat.label.clone()),
        //         color: Some(cat.color.clone()),
        //         cat_index: *cat_index,
        //         data_index: i,
        //     })
    }
    // for cat_points in ordered_points {
    //     points.extend(cat_points);
    // }
    LineData {
        lines,
        x: x_axis,
        y: y_axis,
        categories: cumulative_data.cat_order.clone(),
    }
}

pub fn plot(dimensions: Dimensions, line_data: LineData, options: &cli::PlotOptions) -> Document {
    let height = dimensions.height
        + dimensions.margin.top
        + dimensions.margin.bottom
        + dimensions.padding.top
        + dimensions.padding.bottom;

    let width = dimensions.width
        + dimensions.margin.right
        + dimensions.margin.left
        + dimensions.padding.right
        + dimensions.padding.left;
    let x_opts = line_data.x.clone();
    let y_opts = line_data.y.clone();

    let cumulative = Chart {
        axes: ChartAxes {
            x: Some(x_opts.clone()),
            y: Some(y_opts.clone()),
            ..Default::default()
        },
        line_data: Some(line_data.clone()),
        dimensions: dimensions.clone(),
        ..Default::default()
    };

    let legend_x = match options.show_legend {
        ShowLegend::Compact => width - 240.0,
        _ => width - 185.0,
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
        .add(cumulative.svg(0.0, 0.0, None).set(
            "transform",
            format!(
                "translate({}, {})",
                dimensions.margin.left, dimensions.margin.top
            ),
        ))
        .add(
            category_legend_full(line_data.categories.clone(), options.show_legend.clone()).set(
                "transform",
                format!(
                    "translate({}, {})",
                    legend_x,
                    height
                        - dimensions.margin.bottom
                        - dimensions.padding.bottom * 2.0
                        - line_data.categories.len() as f64 * 26.0
                ),
            ),
        );

    document
}
