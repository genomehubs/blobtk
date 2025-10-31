use std::cmp::max;
use std::env;
use std::f64::consts::PI;

use coord_transforms::d2::polar2cartesian;
use coord_transforms::prelude::*;
use num_integer::div_rem;
use svg::node::element::path::Data;
use svg::node::element::{Circle, Group, Line, Path, Rectangle, Text};
use svg::node::Text as nodeText;

use crate::utils::{
    format_si, linear_scale, linear_scale_float, min_float, scale_float, scale_floats,
};

use super::axis::{AxisOptions, Position, Scale, TickOptions, TickStatus};
use super::style::path_open;

#[derive(Clone, Debug)]
pub struct RadialTick {
    pub index: usize,
    pub offset: f64,
    pub angle: f64,
    pub label: Text,
    pub path: Path,
    pub outer_label: Text,
    pub midpoint: (f64, f64),
    pub status: TickStatus,
}

#[derive(Clone, Debug)]
pub struct Tick {
    pub label: Text,
    pub label_width: f64,
    pub path: Path,
    pub position: f64,
    pub gridline: Path,
    pub status: TickStatus,
}

impl Default for Tick {
    fn default() -> Tick {
        Tick {
            label: Text::new(),
            path: Path::new(),
            label_width: 0.0,
            position: 0.0,
            gridline: Path::new(),
            status: TickStatus::Major,
        }
    }
}

#[derive(Clone, Debug)]
pub enum LegendShape {
    Rect,
    Circumference,
    Radius,
    None,
}

pub struct LegendEntry {
    pub title: String,
    pub subtitle: Option<String>,
    pub color: String,
    pub shape: LegendShape,
}

impl Default for LegendEntry {
    fn default() -> LegendEntry {
        LegendEntry {
            title: "".to_string(),
            subtitle: None,
            color: "#000000".to_string(),
            shape: LegendShape::Rect,
        }
    }
}

pub fn font_family(fallback: &str) -> String {
    match env::var("FONT_FAMILY") {
        Ok(family) => format!("{}, {}", family, fallback),
        _ => fallback.to_string(),
    }
}

pub fn legend_group(
    title: String,
    entries: Vec<LegendEntry>,
    subtitle: Option<String>,
    columns: u8,
) -> Group {
    let processed_font_family = font_family("Roboto, Open sans, DejaVu Sans, Arial, sans-serif");
    let title_text = if title.is_empty() {
        Text::new()
    } else {
        Text::new()
            .set("font-family", processed_font_family.clone())
            .set("font-size", "24")
            .set("text-anchor", "start")
            .set("dominant-baseline", "bottom")
            .set("stroke", "none")
            .set("fill", "black")
            .add(nodeText::new(title.clone()))
    };
    let mut group = Group::new().add(title_text);
    let cell: i32 = 18;
    let gap = 8;
    let mut offset_y = 0;
    let mut offset_x: i32 = -175;
    let per_column = entries.len() / columns as usize;
    for (i, entry) in entries.iter().enumerate() {
        if i % per_column == 0 {
            offset_x += 175;
            offset_y = if title.is_empty() { 0 } else { gap / 2 };
        }
        let title_width = cell + gap + entry.title.len() as i32 * cell * 11 / 20;
        let mut rect_width = title_width;
        let (anchor, position, rect_x) = match entry.subtitle {
            Some(_) => {
                rect_width += gap + entry.subtitle.clone().unwrap().len() as i32 * cell * 11 / 20;
                ("end", -gap, -gap - title_width)
            }
            None => ("start", cell + gap, -gap / 2),
        };
        let entry_text = Text::new()
            .set("font-family", processed_font_family.clone())
            .set("font-size", cell)
            .set("text-anchor", anchor)
            .set("dominant-baseline", "bottom")
            .set("stroke", "none")
            .set("fill", "black")
            .set("x", position)
            .set("y", cell + gap / 2)
            .add(nodeText::new(&entry.title));
        let entry_subtext = if entry.subtitle.is_some() {
            Text::new()
                .set("font-family", processed_font_family.clone())
                .set("font-size", cell as f64 * 0.9)
                .set("text-anchor", "start")
                .set("dominant-baseline", "bottom")
                .set("stroke", "none")
                .set("fill", "black")
                .set("x", cell + gap)
                .set("y", cell * 9 / 10 + gap / 2)
                .add(nodeText::new(entry.subtitle.clone().unwrap()))
        } else {
            Text::new()
        };
        let background = Group::new().add(
            Rectangle::new()
                .set("stroke", "none")
                .set("fill", "#ffffff")
                .set("x", rect_x)
                .set("y", gap / 2)
                .set("height", cell + gap)
                .set("width", rect_width)
                .set("opacity", 0.95),
        );
        let shape = match entry.shape {
            LegendShape::Rect => Group::new().add(
                Rectangle::new()
                    .set("stroke", "black")
                    .set("stroke-width", 2)
                    .set("fill", entry.color.clone())
                    .set("x", 0)
                    .set("y", 6)
                    .set("height", cell)
                    .set("width", cell),
            ),
            LegendShape::Circumference => Group::new()
                .add(
                    Circle::new()
                        .set("stroke", "black")
                        .set("stroke-width", 2)
                        .set("fill", entry.color.clone())
                        .set("cx", cell / 2)
                        .set("cy", 6 + cell / 2)
                        .set("r", cell / 2),
                )
                .add(
                    Line::new()
                        .set("fill", "none")
                        .set("stroke", "black")
                        .set("stroke-width", 1)
                        .set("x1", cell / 2)
                        .set("y1", 6 + cell / 2)
                        .set("x2", cell / 2)
                        .set("y2", 6),
                ),
            LegendShape::Radius => Group::new()
                .add(
                    Circle::new()
                        .set("stroke", "black")
                        .set("stroke-width", 1)
                        .set("fill", entry.color.clone())
                        .set("cx", cell / 2)
                        .set("cy", 6 + cell / 2)
                        .set("r", cell / 2),
                )
                .add(
                    Line::new()
                        .set("fill", "none")
                        .set("stroke", "black")
                        .set("stroke-width", 2)
                        .set("x1", cell / 2)
                        .set("y1", 6 + cell / 2)
                        .set("x2", cell / 2)
                        .set("y2", 6),
                ),
            LegendShape::None => Group::new(),
        };
        let entry_group = Group::new()
            .set(
                "transform",
                format!("translate({}, {})", offset_x, offset_y),
            )
            .add(background)
            .add(shape)
            .add(entry_text)
            .add(entry_subtext);
        group = group.add(entry_group);
        offset_y = offset_y + cell + gap;
    }
    match subtitle {
        None => (),
        Some(subtitle_string) => {
            let subtitle_text = Text::new()
                .set("font-family", processed_font_family.clone())
                .set("font-size", "18")
                .set("text-anchor", "start")
                .set("dominant-baseline", "bottom")
                .set("stroke", "none")
                .set("fill", "black")
                .set("transform", "translate(100, 0)")
                .add(nodeText::new(subtitle_string));
            group = group.add(subtitle_text);
        }
    }

    group
}

pub fn path_axis_major(path_data: Data, color: Option<&str>, width: Option<f64>) -> Path {
    let col = color.unwrap_or("black");
    Path::new()
        .set("stroke", col)
        .set("fill", "none")
        .set("stroke-width", width.unwrap_or_else(|| 3.0))
        .set("d", path_data)
}

pub fn path_axis_minor(path_data: Data, color: Option<&str>, width: Option<f64>) -> Path {
    let col = color.unwrap_or("black");
    Path::new()
        .set("stroke", col)
        .set("fill", "none")
        .set("stroke-width", width.unwrap_or_else(|| 1.0))
        .set("d", path_data)
}

pub fn path_gridline_major(path_data: Data, color: Option<&str>) -> Path {
    let col = color.unwrap_or("black");
    Path::new()
        .set("stroke", col)
        .set("fill", "none")
        .set("stroke-width", 2)
        .set("d", path_data)
}

pub fn path_gridline_minor(path_data: Data, color: Option<&str>) -> Path {
    let col = color.unwrap_or("black");
    Path::new()
        .set("stroke", col)
        .set("fill", "none")
        .set("stroke-width", 1)
        .set("stroke-dasharray", "5, 5")
        .set("d", path_data)
}

pub fn set_tick(
    value: f64,
    label: String,
    domain: &[f64; 2],
    range: &[f64; 2],
    status: &TickStatus,
    scale: &String,
) -> Tick {
    let offset = scale_float(value, &domain, &range, &scale, None);
    let path = match status {
        TickStatus::Major => path_axis_major(
            Data::new().move_to((-10, offset)).line_to((0, offset)),
            None,
            None,
        ),
        TickStatus::Minor => path_axis_minor(
            Data::new().move_to((-5, offset)).line_to((0, offset)),
            None,
            None,
        ),
    };
    let text = match status {
        TickStatus::Major => Text::new()
            .set(
                "font-family",
                font_family("Roboto, Open sans, DejaVu Sans, Arial, sans-serif"),
            )
            .set("font-size", "20")
            .set("text-anchor", "end")
            .set("dominant-baseline", "middle")
            .set("stroke", "none")
            .set("fill", "black")
            .set("transform", format!("translate({:?}, {:?})", -15, offset,))
            .add(nodeText::new(label.clone())),
        TickStatus::Minor => Text::new(),
    };

    Tick {
        label: text,
        path,
        label_width: label.len() as f64 * 11.0, // Assuming an average character width of 10.0
        position: offset,
        status: match status {
            TickStatus::Major => TickStatus::Major,
            TickStatus::Minor => TickStatus::Minor,
        },
        ..Default::default()
    }
}

pub fn create_tick(
    value: f64,
    label: String,
    range: &[f64; 2],
    axis_options: &AxisOptions,
    tick_options: &TickOptions,
) -> Tick {
    let location = scale_floats(
        value,
        &axis_options.domain,
        &range,
        &axis_options.scale,
        None,
    );
    let axis_height = axis_options.height;
    let rotate = axis_options.rotate;
    let (x1, y1, x2, y2, x3, y3, x_text, y_text, anchor, baseline, angle) =
        match axis_options.position {
            Position::TOP => (
                location,
                axis_options.offset,
                location,
                axis_options.offset - tick_options.length,
                location,
                axis_height,
                location,
                axis_options.offset - tick_options.length * 1.5,
                if rotate { "end" } else { "middle" },
                if rotate { "central" } else { "auto" },
                if rotate { 90.0 } else { 0.0 },
            ),
            Position::RIGHT => (
                axis_options.offset,
                location,
                axis_options.offset + tick_options.length,
                location,
                axis_height,
                location,
                axis_options.offset + tick_options.length * 1.5,
                location,
                "middle",
                "hanging",
                270.0,
            ),
            Position::BOTTOM => (
                location,
                axis_options.offset,
                location,
                axis_options.offset + tick_options.length,
                location,
                axis_options.offset - axis_height,
                location,
                axis_options.offset + tick_options.length * 1.5,
                if rotate { "start" } else { "middle" },
                if rotate { "central" } else { "hanging" },
                if rotate { 90.0 } else { 0.0 },
            ),
            Position::LEFT => (
                axis_options.offset,
                location,
                axis_options.offset - tick_options.length,
                location,
                axis_options.offset + axis_height,
                location,
                axis_options.offset - tick_options.length * 1.5,
                location,
                if rotate { "end" } else { "middle" },
                if rotate { "central" } else { "hanging" },
                if rotate { 0.0 } else { 90.0 },
            ),
        };
    let path_data = Data::new().move_to((x1, y1)).line_to((x2, y2));
    let path = match tick_options.status {
        TickStatus::Major => path_axis_major(
            path_data,
            Some(&axis_options.color),
            Some(tick_options.weight),
        ),
        TickStatus::Minor => path_axis_minor(
            path_data,
            Some(&axis_options.color),
            Some(tick_options.weight),
        ),
    };
    let gridline = match tick_options.status {
        TickStatus::Major => path_open(
            Data::new().move_to((x1, y1)).line_to((x3, y3)),
            Some("#cccccc"),
            Some(1.0),
        ),
        _ => Path::new(),
    };
    let text = if axis_options.tick_labels && !label.is_empty() {
        // match tick_options.status {
        Text::new()
            .set(
                "font-family",
                font_family("Roboto, Open sans, DejaVu Sans, Arial, sans-serif"),
            )
            .set("font-size", tick_options.font_size)
            .set("text-anchor", anchor)
            .set("dominant-baseline", baseline)
            .set("stroke", "none")
            .set("fill", axis_options.color.clone())
            .set(
                "transform",
                format!("translate({:?}, {:?}) rotate({:?})", x_text, y_text, angle),
            )
            .add(nodeText::new(&label)) //,
                                        // TickStatus::Minor => Text::new(),
                                        // }
    } else {
        Text::new()
    };

    Tick {
        label: text,
        label_width: label.len() as f64 * 11.0, // Assuming an average character width of 10.0
        path,
        position: location,
        gridline,
        status: tick_options.status.clone(),
        ..Default::default()
    }
}

pub fn create_axis_ticks(options: &AxisOptions, status: TickStatus) -> Vec<Tick> {
    let range = [
        options.range[0] + options.padding[0],
        options.range[1] + options.padding[0],
    ];
    let domain = options.domain;

    let mut power: i32 = 0;
    let mut min_value = domain[0].clone().abs();
    let mut min_val = domain[0].clone().abs();
    if min_val == 0.0 && options.clamp.is_some() {
        min_value = options.clamp.unwrap().clone();
        min_val = options.clamp.unwrap().clone();
    }

    if min_val > 1.0 {
        while min_val > 1.0 {
            power += 1;
            min_val /= 10.0;
        }
    } else if min_val > 0.0 {
        while min_val < 1.0 {
            power -= 1;
            min_val *= 10.0;
        }
    }
    let target = options.tick_count as f64;
    let mut ticks: Vec<Tick> = vec![];
    match options.scale {
        Scale::LOG => {
            let diff = domain[1].log10() - min_value.log10();
            let step = if diff > 11.0 { 100.0 } else { 10.0 };
            match status {
                TickStatus::Major => {
                    let mut i = 10u32.pow(power.abs() as u32) as f64;
                    if power < 0 {
                        i = 1.0 / i;
                    }
                    if min_value.clone() < 0.0 {
                        i = -i
                    }
                    while i <= domain[1].clone() {
                        let label = if i >= min_value.clone() {
                            format_si(&i, 3, None)
                        } else {
                            String::new()
                        };
                        ticks.push(create_tick(
                            i,
                            label,
                            &range,
                            &options,
                            &options.major_ticks.as_ref().unwrap(),
                        ));
                        i = i * step;
                    }
                }
                TickStatus::Minor => {
                    let mut i = 10u32.pow((max(0, power.abs() - 1)) as u32) as f64;
                    if power < 0 {
                        i = 1.0 / i;
                    }
                    if min_value.clone() < 0.0 {
                        i = -i
                    }
                    while i <= domain[1].clone() {
                        let mut j = i * 2.0;
                        while j < i * 10.0 && j <= domain[1].clone() {
                            let label = if j >= min_value.clone() {
                                format_si(&j, 3, None)
                            } else {
                                String::new()
                            };
                            if j as f64 >= min_value {
                                ticks.push(create_tick(
                                    j,
                                    label,
                                    &range,
                                    &options,
                                    &options.minor_ticks.as_ref().unwrap(),
                                ));
                            }
                            j = j + i;
                        }
                        i = i * 10.0;
                    }
                }
            }
        }
        Scale::LINEAR => {
            let diff = domain[1] - min_value;
            // let round_step =
            let divisor = 0.1
                * if power < 0 {
                    1.0 / 10u32.pow(power.abs() as u32) as f64
                } else {
                    10u32.pow(power.abs() as u32) as f64
                };
            let mut step = divisor.clone();
            let steps = [2.0, 2.5, 5.0, 10.0];
            let mut multiple = 1.0;
            while diff / step > target {
                for (i, _) in steps.iter().enumerate() {
                    step = divisor * steps[i] * multiple;
                    if diff / step <= target {
                        break;
                    }
                }
                multiple *= 10.0;
            }

            match status {
                TickStatus::Major => {
                    let mut i = step * (min_value / step).ceil();
                    while i <= domain[1].clone() {
                        let label = if i >= min_value.clone() {
                            format_si(&i, 3, None)
                        } else {
                            String::new()
                        };
                        ticks.push(create_tick(
                            i,
                            label,
                            &range,
                            &options,
                            &options.major_ticks.as_ref().unwrap(),
                        ));
                        i += step;
                    }
                }
                TickStatus::Minor => {}
            }
        }
        _ => {}
    };

    ticks
}

pub fn set_axis_ticks(
    max_value: &f64,
    min_value: &f64,
    status: &TickStatus,
    dimension: &f64,
    scale: &String,
) -> Vec<Tick> {
    let range = [-dimension.clone(), 0.0];
    let domain = [min_value.clone(), max_value.clone()];

    if scale == &"scaleLog".to_string() {}
    let mut power: i32 = 0;
    let mut min_val = min_value.clone().abs();

    if min_val > 1.0 {
        while min_val > 1.0 {
            power += 1;
            min_val /= 10.0;
        }
    } else {
        while min_val < 1.0 {
            power -= 1;
            min_val *= 10.0;
        }
    }

    let mut ticks: Vec<Tick> = vec![];
    match scale.as_str() {
        "scaleLinear" => {
            // Distribute ticks on nice round numbers for linear scale
            let tick_count = 6; // You can make this configurable if needed
            set_linear_ticks(
                max_value, min_value, status, scale, range, domain, &mut ticks, tick_count,
            );
        }
        "scaleLog" | "scaleSqrt" => match status {
            TickStatus::Major => {
                let mut i = 10u32.pow(power.abs() as u32) as f64;
                if power < 0 {
                    i = 1.0 / i;
                }
                if min_value.clone() < 0.0 {
                    i = -i
                }
                while i <= max_value.clone() {
                    let label = if i > min_value.clone() {
                        format_si(&i, 3, None)
                    } else {
                        String::new()
                    };
                    ticks.push(set_tick(i, label, &domain, &range, &status, &scale));
                    i = i * 10.0;
                }
            }
            TickStatus::Minor => {
                let mut i = 10u32.pow((max(0, power.abs() - 1)) as u32) as f64;
                if power < 0 {
                    i = 1.0 / i;
                }
                if min_value.clone() < 0.0 {
                    i = -i
                }
                while i <= max_value.clone() {
                    let mut j = i * 2.0;
                    while j < i * 10.0 && j <= max_value.clone() {
                        if &(j as f64) >= min_value {
                            ticks.push(set_tick(
                                j,
                                String::new(),
                                &domain,
                                &range,
                                &status,
                                &scale,
                            ));
                        }
                        j = j + i;
                    }
                    ticks.push(set_tick(i, String::new(), &domain, &range, &status, &scale));
                    i = i * 10.0;
                }
            }
        },
        _ => {}
    }
    ticks
}

fn set_linear_ticks(
    max_value: &f64,
    min_value: &f64,
    status: &TickStatus,
    scale: &String,
    range: [f64; 2],
    domain: [f64; 2],
    ticks: &mut Vec<Tick>,
    tick_count: i32,
) {
    let diff = max_value - min_value;
    if diff == 0.0 {
        // Edge case: no range
        match status {
            TickStatus::Major => {
                ticks.push(set_tick(
                    *min_value,
                    format_si(min_value, 3, None),
                    &domain,
                    &range,
                    &status,
                    &scale,
                ));
            }
            TickStatus::Minor => {}
        }
    } else {
        // Find a "nice" step size
        let raw_step = diff / tick_count as f64;
        let magnitude = 10f64.powf(raw_step.abs().log10().floor());
        let nice_steps = [1.0, 2.0, 2.5, 5.0, 10.0];
        let mut step = nice_steps[0] * magnitude;
        for s in &nice_steps {
            let candidate = s * magnitude;
            if diff / candidate <= tick_count as f64 {
                step = candidate;
                break;
            }
        }
        // Find the first tick >= min_value that is a multiple of step
        let first_tick = (min_value / step).ceil() * step;
        match status {
            TickStatus::Major => {
                let mut value = first_tick;
                while value <= *max_value {
                    let label = format_si(&value, 3, None);
                    ticks.push(set_tick(value, label, &domain, &range, &status, &scale));
                    value += step;
                }
            }
            TickStatus::Minor => {
                // Optionally add minor ticks between major ticks
                let minor_tick_count = 4;
                let minor_step = step / (minor_tick_count as f64 + 1.0);
                let mut value = first_tick - step;
                while value <= *max_value - step {
                    for j in 1..=minor_tick_count {
                        let minor_value = value + j as f64 * minor_step;
                        if minor_value > *min_value - step * 0.5
                            && minor_value < *max_value + step * 0.5
                        {
                            ticks.push(set_tick(
                                minor_value,
                                String::new(),
                                &domain,
                                &range,
                                &status,
                                &scale,
                            ));
                        }
                    }
                    value += step;
                }
            }
        }
    }
}

pub fn set_tick_circular(
    index: usize,
    offset: f64,
    bin_count: usize,
    max_radians: f64,
    label: String,
    outer_label: String,
    angle_domain: &[f64; 2],
    angle_range: &[f64; 2],
    tick_domain: &[usize; 2],
    tick_range: &[f64; 2],
    status: &TickStatus,
    options: &TickOptions,
) -> RadialTick {
    let angle = linear_scale_float(index as f64 + offset, &angle_domain, &angle_range);
    let processed_font_family = font_family("Roboto, Open sans, DejaVu Sans, Arial, sans-serif");

    let mut adjusted_tick_range = [tick_range[0], tick_range[1]];
    if offset > 0.0 {
        let segment = max_radians / 2.0 / bin_count as f64;
        let theta = PI - segment - PI / 2 as f64;
        let inradius = tick_range[0] as f64 * theta.sin();
        let diff = (0.5 - offset).abs();
        let diff_segment = segment * diff * 2.0;
        let adjusted_radius = inradius / diff_segment.cos();

        let outer_inradius = tick_range[1] as f64 * theta.sin();
        let outer_adjusted_radius = outer_inradius / diff_segment.cos();
        adjusted_tick_range = [adjusted_radius, outer_adjusted_radius];
    }

    let tick_size = match status {
        TickStatus::Major => 5,
        TickStatus::Minor => 3,
    };
    let tick_distances = [
        linear_scale(0, &tick_domain, &adjusted_tick_range),
        linear_scale(tick_size, &tick_domain, &adjusted_tick_range),
        linear_scale(
            tick_domain[1] - tick_size,
            &tick_domain,
            &adjusted_tick_range,
        ),
        linear_scale(tick_domain[1], &tick_domain, &adjusted_tick_range),
    ];
    let midpoint = polar2cartesian(&Vector2::new(
        (adjusted_tick_range[1] - adjusted_tick_range[0]) / 2.0 + adjusted_tick_range[0],
        angle,
    ));
    let tick_points = [
        polar2cartesian(&Vector2::new(tick_distances[0], angle)),
        polar2cartesian(&Vector2::new(tick_distances[1], angle)),
        polar2cartesian(&Vector2::new(tick_distances[2], angle)),
        polar2cartesian(&Vector2::new(tick_distances[3], angle)),
    ];
    let outer_point = polar2cartesian(&Vector2::new(tick_distances[3] + 4.0, angle));
    let tick_path_data = if options.label_ticks {
        Data::new()
            .move_to((tick_points[0][0], tick_points[0][1]))
            .line_to((tick_points[1][0], tick_points[1][1]))
            .move_to((tick_points[2][0], tick_points[2][1]))
            .line_to((tick_points[3][0], tick_points[3][1]))
    } else {
        Data::new()
            .move_to((tick_points[0][0], tick_points[0][1]))
            .line_to((tick_points[1][0], tick_points[1][1]))
    };
    let path = match status {
        TickStatus::Major => path_axis_major(tick_path_data, None, None),
        TickStatus::Minor => path_axis_minor(tick_path_data, None, None),
    };
    let text = if label == "100".to_string() && angle > 1.4 * PI {
        Text::new()
    } else {
        Text::new()
            .set("font-family", processed_font_family.clone())
            .set("font-size", options.font_size.clone())
            .set("text-anchor", "middle")
            .set("dominant-baseline", "middle")
            .set("stroke", "none")
            .set("fill", options.font_color.clone())
            .set(
                "transform",
                format!(
                    "translate({:?}, {:?}) rotate({:?})",
                    midpoint[0],
                    midpoint[1],
                    (angle + PI / 2.0) * 180.0 / PI
                ),
            )
            .add(nodeText::new(label))
    };
    let outer_text = match status {
        TickStatus::Major => Text::new()
            .set("font-family", processed_font_family.clone())
            .set("font-size", "20")
            .set("text-anchor", "middle")
            .set("dominant-baseline", "bottom")
            .set("stroke", "none")
            .set("fill", "black")
            .set(
                "transform",
                format!(
                    "translate({:?}, {:?}) rotate({:?})",
                    outer_point[0],
                    outer_point[1],
                    (angle + PI / 2.0) * 180.0 / PI
                ),
            )
            .add(nodeText::new(outer_label)),
        TickStatus::Minor => Text::new(),
    };

    RadialTick {
        index,
        offset,
        angle,
        label: text,
        outer_label: outer_text,
        path,
        midpoint: (midpoint[0], midpoint[1]),
        status: match status {
            TickStatus::Major => TickStatus::Major,
            TickStatus::Minor => TickStatus::Minor,
        },
    }
}

pub fn set_axis_ticks_circular(
    bin_count: usize,
    tick_count: usize,
    status: TickStatus,
    max_radians: f64,
    radius: f64,
    outer_radius: f64,
    span: usize,
    options: TickOptions,
) -> Vec<RadialTick> {
    let (divisor, remainder) = div_rem(bin_count, tick_count);
    let angle_domain = [0.0, bin_count as f64];
    let angle_range = [-PI / 2.0, max_radians - PI / 2.0];
    let tick_domain = [0, 24];
    let tick_range = [radius, outer_radius];
    let mut ticks: Vec<RadialTick> = vec![];
    if remainder == 0 {
        // if divisor > 1 {
        ticks.push(set_tick_circular(
            0,
            0.0,
            bin_count,
            max_radians,
            String::from("0%"),
            String::new(),
            &angle_domain,
            &angle_range,
            &tick_domain,
            &tick_range,
            &status,
            &options,
        ));
        // }
        for i in (divisor..bin_count + 1).step_by(divisor) {
            let label = format!("{}", ((i) as f64 / bin_count as f64 * 100.0) as u64);
            let outer_label = format_si(
                &(span as f64 / tick_count as f64 * ticks.len() as f64),
                3,
                None,
            );
            ticks.push(set_tick_circular(
                i,
                0.0,
                bin_count,
                max_radians,
                label,
                outer_label,
                &angle_domain,
                &angle_range,
                &tick_domain,
                &tick_range,
                &status,
                &options,
            ));
        }
    } else {
        let mut sum: f64 = 0.0;
        let step = divisor as f64 + remainder as f64 / tick_count as f64;
        ticks.push(set_tick_circular(
            0,
            0.0,
            bin_count,
            max_radians,
            String::from("0%"),
            String::new(),
            &angle_domain,
            &angle_range,
            &tick_domain,
            &tick_range,
            &status,
            &options,
        ));
        while ticks.len() < tick_count + 1 {
            sum += step;
            let adj_sum = sum + 0.001;
            let index = adj_sum.floor() as usize;
            let label = format!("{}", (sum / bin_count as f64 * 100.0).round() as u64);
            let outer_label = format_si(
                &(span as f64 / tick_count as f64 * ticks.len() as f64),
                3,
                None,
            );
            ticks.push(set_tick_circular(
                index,
                ((sum - index as f64).abs() * 1000.0).round() / 1000.0,
                bin_count,
                max_radians,
                label,
                outer_label,
                &angle_domain,
                &angle_range,
                &tick_domain,
                &tick_range,
                &status,
                &options,
            ))
        }
    }
    ticks
}

pub fn arc_path(
    radius: f64,
    inner_radius: Option<f64>,
    min_radians: f64,
    max_radians: f64,
    resolution: usize,
) -> Data {
    let mut path_data = Data::new();

    let step = (max_radians - min_radians) / resolution as f64;
    let length = resolution;

    let first_polar_coord = Vector2::new(radius, min_radians);
    let first_cartesian_coord = polar2cartesian(&first_polar_coord);

    match inner_radius {
        None => path_data = path_data.move_to((first_cartesian_coord[0], first_cartesian_coord[1])),
        Some(rad) => {
            if rad == 0.0 {
                let polar_coord = Vector2::new(0.0, 0.0);
                let cartesian_coord = polar2cartesian(&polar_coord);
                path_data = path_data
                    .move_to((cartesian_coord[0], cartesian_coord[1]))
                    .line_to((first_cartesian_coord[0], first_cartesian_coord[1]));
            } else {
                let mut angle = max_radians;
                for i in (0..length + 1).rev() {
                    let polar_coord = Vector2::new(rad, angle);
                    let cartesian_coord = polar2cartesian(&polar_coord);
                    if i == length {
                        path_data = path_data.move_to((cartesian_coord[0], cartesian_coord[1]))
                    } else {
                        path_data = path_data.line_to((cartesian_coord[0], cartesian_coord[1]))
                    }
                    angle -= step;
                }
                path_data = path_data.line_to((first_cartesian_coord[0], first_cartesian_coord[1]))
            }
        }
    };

    let mut angle = min_radians;
    for _ in 0..(length + 1) {
        let polar_coord = Vector2::new(radius, angle);
        let cartesian_coord = polar2cartesian(&polar_coord);
        path_data = path_data.line_to((cartesian_coord[0], cartesian_coord[1]));
        angle += step;
    }

    match inner_radius {
        None => (),
        Some(_) => path_data = path_data.close(),
    };

    path_data
}

pub fn polar_to_path(
    polar_coords: &Vec<Vec<f64>>,
    radius: f64,
    bin_count: usize,
    max_radians: f64,
) -> Data {
    let mut path_data = Data::new();

    let mut length = polar_coords.len();
    if length == 0 {
        length = bin_count;
    }

    for i in (0..length).rev() {
        let angle = if polar_coords.len() > 0 {
            polar_coords[i][1]
        } else {
            max_radians * (i + 1) as f64 / length as f64 - PI / 2.0
        };
        let polar_coord = Vector2::new(radius, angle);
        let cartesian_coord = polar2cartesian(&polar_coord);
        if i == length - 1 {
            path_data = path_data.move_to((cartesian_coord[0], cartesian_coord[1]))
        } else {
            path_data = path_data.line_to((cartesian_coord[0], cartesian_coord[1]))
        }
        if i == 0 {
            let final_polar_coord = Vector2::new(radius, -PI / 2.0);
            let final_cartesian_coord = polar2cartesian(&final_polar_coord);
            path_data = path_data.line_to((final_cartesian_coord[0], final_cartesian_coord[1]))
        }
    }

    for i in 0..polar_coords.len() {
        let polar_coord_end = Vector2::new(polar_coords[i][0], polar_coords[i][1]);
        let polar_coord_start;
        if i > 0 {
            polar_coord_start = Vector2::new(polar_coords[i][0], polar_coords[i - 1][1]);
        } else {
            polar_coord_start = Vector2::new(
                polar_coords[i][0],
                linear_scale(0, &[0, bin_count], &[-PI / 2.0, max_radians - PI / 2.0]),
            );
        };

        let cartesian_start = polar2cartesian(&polar_coord_start);
        let cartesian_end = polar2cartesian(&polar_coord_end);
        path_data = path_data
            .line_to((cartesian_start[0], cartesian_start[1]))
            .line_to((cartesian_end[0], cartesian_end[1]));
    }
    if polar_coords.len() > 0 {
        path_data = path_data.close();
    }
    path_data
}

pub fn polar_to_path_bounded(
    polar_coords: &Vec<Vec<f64>>,
    polar_bound_coords: &Vec<Vec<f64>>,
    bin_count: usize,
    max_radians: f64,
) -> Data {
    let mut path_data = Data::new();

    let mut length = polar_coords.len();
    if length == 0 {
        length = bin_count;
    }

    for i in (0..polar_bound_coords.len()).rev() {
        let polar_coord_end;
        let polar_coord_start;
        if i < polar_bound_coords.len() - 1 {
            polar_coord_start =
                Vector2::new(polar_bound_coords[i + 1][0], polar_bound_coords[i + 1][1]);
            polar_coord_end = Vector2::new(polar_bound_coords[i + 1][0], polar_bound_coords[i][1]);
        } else {
            polar_coord_start = Vector2::new(polar_bound_coords[i][0], max_radians - PI / 2.0);
            polar_coord_end = Vector2::new(polar_bound_coords[i][0], polar_bound_coords[i][1]);
        };

        let cartesian_start = polar2cartesian(&polar_coord_start);
        let cartesian_end = polar2cartesian(&polar_coord_end);
        if i == length - 1 {
            path_data = path_data
                .move_to((cartesian_start[0], cartesian_start[1]))
                .line_to((cartesian_end[0], cartesian_end[1]));
        } else {
            path_data = path_data
                .line_to((cartesian_start[0], cartesian_start[1]))
                .line_to((cartesian_end[0], cartesian_end[1]));
        }
        if i == 0 {
            let final_polar_coord = Vector2::new(polar_bound_coords[i][0], -PI / 2.0);
            let final_cartesian_coord = polar2cartesian(&final_polar_coord);
            path_data = path_data.line_to((final_cartesian_coord[0], final_cartesian_coord[1]))
        }
    }

    for i in 0..polar_coords.len() {
        let polar_coord_end = Vector2::new(polar_coords[i][0], polar_coords[i][1]);
        let polar_coord_start;
        if i > 0 {
            polar_coord_start = Vector2::new(polar_coords[i][0], polar_coords[i - 1][1]);
        } else {
            polar_coord_start = Vector2::new(
                polar_coords[i][0],
                linear_scale(0, &[0, bin_count], &[-PI / 2.0, max_radians - PI / 2.0]),
            );
        };

        let cartesian_start = polar2cartesian(&polar_coord_start);
        let cartesian_end = polar2cartesian(&polar_coord_end);
        path_data = path_data
            .line_to((cartesian_start[0], cartesian_start[1]))
            .line_to((cartesian_end[0], cartesian_end[1]));
    }
    if polar_coords.len() > 0 {
        path_data = path_data.close();
    }
    path_data
}

pub fn chart_axis(plot_axis: &AxisOptions) -> (Group, Group) {
    let mut major_tick_group = Group::new();
    let mut major_gridline_group = Group::new();
    let mut major_tick_count = 0;
    if plot_axis.major_ticks.is_some() {
        let major_ticks = create_axis_ticks(&plot_axis, TickStatus::Major);
        major_tick_count = major_ticks.len();
        if major_tick_count > 0 {
            add_ticks_to_axis(
                major_ticks,
                &mut major_tick_group,
                &mut major_gridline_group,
            );
        }
    };

    let mut minor_tick_group = Group::new();
    if plot_axis.minor_ticks.is_some() {
        let minor_ticks = create_axis_ticks(&plot_axis, TickStatus::Minor);
        if major_tick_count == 0 {
            add_ticks_to_axis(
                minor_ticks,
                &mut minor_tick_group,
                &mut major_gridline_group,
            );
        } else {
            for tick in minor_ticks {
                minor_tick_group = minor_tick_group.add(tick.path);
            }
        }
        // for tick in minor_ticks {
        //     minor_tick_group = if major_tick_count == 0 {
        //         major_gridline_group = major_gridline_group.add(tick.gridline);
        //         minor_tick_group.add(tick.path).add(tick.label)
        //     } else {
        //         minor_tick_group.add(tick.path)
        //     };
        // }
    }

    let (x1, y1, x2, y2, label_x, label_y, label_rotate) = match plot_axis.position {
        Position::TOP => (
            plot_axis.range[0],
            plot_axis.offset,
            plot_axis.range[1] + plot_axis.padding[0] + plot_axis.padding[1],
            plot_axis.offset,
            (plot_axis.range[1] + plot_axis.range[0]) / 2.0 + plot_axis.padding[0],
            plot_axis.offset - plot_axis.label_offset,
            0.0,
        ),
        Position::RIGHT => (
            plot_axis.offset,
            plot_axis.range[1],
            plot_axis.offset,
            plot_axis.range[0] + plot_axis.padding[0] + plot_axis.padding[1],
            plot_axis.offset + plot_axis.label_offset,
            (plot_axis.range[1] + plot_axis.range[0]) / 2.0 + plot_axis.padding[0],
            90.0,
        ),
        Position::BOTTOM => (
            plot_axis.range[0],
            plot_axis.offset,
            plot_axis.range[1] + plot_axis.padding[0] + plot_axis.padding[1],
            plot_axis.offset,
            (plot_axis.range[1] + plot_axis.range[0]) / 2.0 + plot_axis.padding[0],
            plot_axis.offset + plot_axis.label_offset,
            0.0,
        ),
        Position::LEFT => (
            plot_axis.offset,
            plot_axis.range[1],
            plot_axis.offset,
            plot_axis.range[0] + plot_axis.padding[0] + plot_axis.padding[1],
            plot_axis.offset - plot_axis.label_offset,
            (plot_axis.range[1] + plot_axis.range[0]) / 2.0 + plot_axis.padding[0],
            90.0,
        ),
    };

    let axis = Line::new()
        .set("fill", "none")
        .set("stroke", "black")
        .set("stroke-width", plot_axis.weight)
        .set("stroke-linecap", "round")
        .set("x1", x1)
        .set("y1", y1)
        .set("x2", x2)
        .set("y2", y2);

    let label = Text::new()
        .set(
            "font-family",
            font_family("Roboto, Open sans, DejaVu Sans, Arial, sans-serif"),
        )
        .set("font-size", plot_axis.font_size)
        .set("text-anchor", "middle")
        .set("dominant-baseline", "middle")
        .set("stroke", "none")
        .set("fill", plot_axis.color.clone())
        .set(
            "transform",
            format!(
                "translate({:?}, {:?}) rotate({:?})",
                label_x, label_y, label_rotate
            ),
        )
        .add(nodeText::new(plot_axis.label.clone()));

    (
        Group::new()
            .add(minor_tick_group)
            .add(major_tick_group)
            .add(axis)
            .add(label),
        Group::new().add(major_gridline_group),
    )
}

fn add_ticks_to_axis(
    major_ticks: Vec<Tick>,
    major_tick_group: &mut Group,
    major_gridline_group: &mut Group,
) {
    let mut first_position = major_ticks[0].position;
    let mut last_position = major_ticks[major_ticks.len() - 1].position;
    let mut major_ticks = major_ticks;
    if first_position > last_position {
        major_ticks.reverse();
        last_position = first_position;
        first_position = major_ticks[0].position;
    }
    let mut previous_position = first_position;
    for (i, tick) in major_ticks.iter().enumerate() {
        let show_tick = if i == 0 {
            true
        } else {
            let mut diff = tick.position - previous_position;
            if i < major_ticks.len() - 1 {
                diff = min_float(diff, last_position - tick.position);
            }
            diff > tick.label_width
        };
        *major_tick_group = major_tick_group.clone().add(tick.path.clone());
        if show_tick {
            previous_position = tick.position;
            *major_tick_group = major_tick_group.clone().add(tick.label.clone());
        }
        *major_gridline_group = major_gridline_group.clone().add(tick.gridline.clone());
    }
}
