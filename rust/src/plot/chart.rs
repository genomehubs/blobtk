use svg::node::element::{Circle, Group};

use super::{
    axis::ChartAxes,
    component::chart_axis,
    data::{HistogramData, LineData, ScatterData},
    style::{path_filled, path_open},
};

#[derive(Clone, Copy, Debug)]
pub struct TopRightBottomLeft {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Default for TopRightBottomLeft {
    fn default() -> TopRightBottomLeft {
        TopRightBottomLeft {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Dimensions {
    pub height: f64,
    pub width: f64,
    pub margin: TopRightBottomLeft,
    pub padding: TopRightBottomLeft,
}

impl Default for Dimensions {
    fn default() -> Dimensions {
        Dimensions {
            height: 900.0,
            width: 900.0,
            margin: TopRightBottomLeft {
                top: 10.0,
                right: 10.0,
                bottom: 100.0,
                left: 100.0,
            },
            padding: TopRightBottomLeft {
                top: 50.0,
                right: 50.0,
                bottom: 50.0,
                left: 50.0,
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct Chart {
    pub axes: ChartAxes,
    pub scatter_data: Option<ScatterData>,
    // pub bar_data: Option<BarData>,
    pub line_data: Option<LineData>,
    pub histogram_data: Option<Vec<HistogramData>>,
    pub line_options: Vec<(String, String)>,
    pub scatter_options: Vec<(String, String)>,
    pub histogram_options: Vec<(String, String)>,
    pub dimensions: Dimensions,
}

impl Default for Chart {
    fn default() -> Chart {
        Chart {
            axes: ChartAxes {
                x: None,
                y: None,
                x2: None,
                y2: None,
            },
            line_data: None,
            scatter_data: None,
            histogram_data: None,
            line_options: vec![],
            scatter_options: vec![],
            histogram_options: vec![],
            dimensions: Dimensions {
                ..Default::default()
            },
        }
    }
}

impl Chart {
    pub fn svg(self, x_offset: f64, y_offset: f64, opacity: Option<f64>) -> Group {
        let opacity = match opacity {
            Some(o) => o,
            None => 1.0,
        };
        let mut group = Group::new();
        let mut axis_group = Group::new();
        let mut gridline_group = Group::new();

        if self.axes.x.is_some() {
            let (axis, gridlines) = chart_axis(self.axes.x.as_ref().unwrap());
            axis_group = axis_group.add(axis);
            gridline_group = gridline_group.add(gridlines);
        }
        if self.axes.y.is_some() {
            let (axis, gridlines) = chart_axis(self.axes.y.as_ref().unwrap());
            axis_group = axis_group.add(axis);
            gridline_group = gridline_group.add(gridlines);
        }
        if self.axes.x2.is_some() {
            let (axis, gridlines) = chart_axis(self.axes.x2.as_ref().unwrap());
            axis_group = axis_group.add(axis);
            gridline_group = gridline_group.add(gridlines);
        }
        if self.axes.y2.is_some() {
            let (axis, gridlines) = chart_axis(self.axes.y2.as_ref().unwrap());
            axis_group = axis_group.add(axis);
            gridline_group = gridline_group.add(gridlines);
        }

        group = group.add(gridline_group);

        if self.scatter_data.is_some() {
            let scatter_data = self.scatter_data.unwrap();
            let mut scatter_group = Group::new();
            for point in scatter_data.points.iter() {
                scatter_group = scatter_group.add(
                    Circle::new()
                        .set("cx", point.x)
                        .set("cy", point.y)
                        .set("r", point.z)
                        .set(
                            "fill",
                            point.color.clone().unwrap_or_else(|| "#ffffff".to_string()),
                        )
                        .set("stroke", "#aaaaaa")
                        .set("fill-opacity", opacity),
                );
            }
            group = group.add(scatter_group.set(
                "transform",
                format!(
                    "translate({}, {})",
                    self.dimensions.padding.left + x_offset,
                    self.dimensions.padding.top + y_offset,
                ),
            ));
        }
        if self.histogram_data.is_some() {
            let mut hist_group = Group::new();
            let mut hist_paths = vec![];
            for hist in self.histogram_data.unwrap() {
                let color;
                color = hist.category.clone().unwrap().color;
                let path_data = hist
                    .clone()
                    .to_path_data(self.axes.x.clone().unwrap().position, true);
                hist_group = hist_group
                    .add(path_filled(path_data.clone(), Some(&color)).set("opacity", opacity));
                hist_paths.push((path_data, color));
            }
            for (path, color) in hist_paths {
                hist_group = hist_group.add(path_open(path, Some(&color), Some(2.0)));
            }
            group = group.add(hist_group.set(
                "transform",
                format!(
                    "translate({}, {})",
                    self.dimensions.padding.left, self.dimensions.padding.bottom
                ),
            ));
        }
        if self.line_data.is_some() {
            let mut line_group = Group::new();
            let line_data = self.line_data.unwrap();
            for line in line_data.lines.iter() {
                let color = match line.color.clone() {
                    Some(col) => col.clone(),
                    None => "#000000".to_string(),
                };
                let path_data = line
                    .clone()
                    .to_path_data(self.axes.x.clone().unwrap().position, true);
                line_group = line_group.add(
                    path_open(path_data.clone(), Some(&color), Some(line.weight))
                        .set("stroke-linecap", "round")
                        .set("stroke-linejoin", "round"),
                );
            }
            group = group.add(line_group.set(
                "transform",
                format!(
                    "translate({}, {})",
                    self.dimensions.padding.left, self.dimensions.padding.bottom
                ),
            ));
        }
        group = group.add(axis_group);
        group
    }
}
