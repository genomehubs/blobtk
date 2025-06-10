use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use pyo3::prelude::*;

use crate::cli::Origin;
use crate::cli::Palette;
use crate::cli::RoundingStrategyWrapper;
use crate::cli::Shape;
use crate::cli::View;
use crate::plot::axis::Scale;
use crate::plot::data::Reducer;
use crate::plot::ShowLegend;

pub fn extract_to_option_list(
    py: Python<'_>,
    map: &HashMap<String, PyObject>,
    key: &str,
) -> Option<HashSet<Vec<u8>>> {
    let hash_key = String::from(key);
    let option: Option<HashSet<Vec<u8>>> = match map.get(&hash_key) {
        Some(value) => {
            let list: Vec<String> = value.extract(py).unwrap();
            let mut unique_values = HashSet::new();
            for item in list {
                unique_values.insert(item.as_bytes().to_vec());
            }
            Some(unique_values)
        }
        _ => None,
    };
    option
}

pub fn extract_to_option_pathbuf(
    py: Python<'_>,
    map: &HashMap<String, PyObject>,
    key: &str,
) -> Option<PathBuf> {
    let hash_key = String::from(key);
    let option: Option<PathBuf> = map
        .get(&hash_key)
        .map(|value| value.extract::<PathBuf>(py).unwrap());
    option
}

pub fn extract_to_pathbuf(py: Python<'_>, map: &HashMap<String, PyObject>, key: &str) -> PathBuf {
    let hash_key = String::from(key);
    let value: PathBuf = map
        .get(&hash_key)
        .map(|value| value.extract::<PathBuf>(py).unwrap())
        .unwrap();
    value
}

pub fn extract_to_default_string(
    py: Python<'_>,
    map: &HashMap<String, PyObject>,
    key: &str,
    default: &str,
) -> String {
    let hash_key = String::from(key);
    let value = match map.get(&hash_key) {
        Some(value) => value.extract::<String>(py).unwrap(),
        _ => String::from(default),
    };
    value
}

pub fn extract_to_option_string(
    py: Python<'_>,
    map: &HashMap<String, PyObject>,
    key: &str,
) -> Option<String> {
    let hash_key = String::from(key);
    let option: Option<String> = map
        .get(&hash_key)
        .map(|value| value.extract::<String>(py).unwrap());
    option
}

pub fn extract_to_option_vec_string(
    py: Python<'_>,
    map: &HashMap<String, PyObject>,
    key: &str,
) -> Option<Vec<String>> {
    let hash_key = String::from(key);
    let option: Option<Vec<String>> = map
        .get(&hash_key)
        .map(|value| value.extract::<Vec<String>>(py).unwrap());
    option
}

pub fn extract_to_bool(py: Python<'_>, map: &HashMap<String, PyObject>, key: &str) -> bool {
    let hash_key = String::from(key);
    let value = match map.get(&hash_key) {
        Some(value) => value.extract::<bool>(py).unwrap(),
        _ => false,
    };
    value
}

pub fn extract_to_usize(py: Python<'_>, map: &HashMap<String, PyObject>, key: &str) -> usize {
    let hash_key = String::from(key);
    let value = match map.get(&hash_key) {
        Some(value) => value.extract::<usize>(py).unwrap(),
        _ => usize::MAX,
    };
    value
}

pub fn extract_to_option_usize(
    py: Python<'_>,
    map: &HashMap<String, PyObject>,
    key: &str,
) -> Option<usize> {
    let hash_key = String::from(key);
    let option: Option<usize> =
        match map
            .get(&hash_key)
            .map(|value| match value.extract::<usize>(py) {
                Ok(val) => Some(val),
                Err(_) => match value.extract::<String>(py).unwrap().parse() {
                    Ok(v) => Some(v),
                    Err(_) => None,
                },
            }) {
            Some(v) => v,
            None => None,
        };
    option
}

pub fn extract_to_option_f64(
    py: Python<'_>,
    map: &HashMap<String, PyObject>,
    key: &str,
) -> Option<f64> {
    let hash_key = String::from(key);
    let option: Option<f64> = match map
        .get(&hash_key)
        .map(|value| match value.extract::<f64>(py) {
            Ok(val) => Some(val),
            Err(_) => match value.extract::<String>(py).unwrap().parse() {
                Ok(v) => Some(v),
                Err(_) => None,
            },
        }) {
        Some(v) => v,
        None => None,
    };
    option
}

pub fn extract_to_option_scale(
    py: Python<'_>,
    map: &HashMap<String, PyObject>,
    key: &str,
) -> Option<Scale> {
    let hash_key = String::from(key);
    let option: Option<Scale> = match map.get(&hash_key) {
        Some(value) => match value.extract::<String>(py).unwrap().parse() {
            Ok(scale) => Some(scale),
            _ => None,
        },
        _ => None,
    };
    option
}

pub fn extract_to_view(py: Python<'_>, map: &HashMap<String, PyObject>, key: &str) -> View {
    let hash_key = String::from(key);
    let value: View = match map.get(&hash_key) {
        Some(value) => match value.extract::<String>(py).unwrap().parse() {
            Ok(view) => view,
            _ => View::Blob,
        },
        _ => View::Blob,
    };
    value
}

pub fn extract_to_option_shape(
    py: Python<'_>,
    map: &HashMap<String, PyObject>,
    key: &str,
) -> Option<Shape> {
    let hash_key = String::from(key);
    let option: Option<Shape> = match map.get(&hash_key) {
        Some(value) => match value.extract::<String>(py).unwrap().parse() {
            Ok(shape) => Some(shape),
            _ => Some(Shape::Circle),
        },
        _ => Some(Shape::Circle),
    };
    option
}

pub fn extract_to_option_reducer(
    py: Python<'_>,
    map: &HashMap<String, PyObject>,
    key: &str,
) -> Option<Reducer> {
    let hash_key = String::from(key);
    let option: Option<Reducer> = match map.get(&hash_key) {
        Some(value) => match value.extract::<String>(py).unwrap().parse() {
            Ok(reducer) => Some(reducer),
            _ => None,
        },
        _ => None,
    };
    option
}

pub fn extract_to_option_showlegend(
    py: Python<'_>,
    map: &HashMap<String, PyObject>,
    key: &str,
) -> Option<ShowLegend> {
    let hash_key = String::from(key);
    let option: Option<ShowLegend> = match map.get(&hash_key) {
        Some(value) => match value.extract::<String>(py).unwrap().parse() {
            Ok(show_legend) => Some(show_legend),
            _ => None,
        },
        _ => None,
    };
    option
}

pub fn extract_to_option_origin(
    py: Python<'_>,
    map: &HashMap<String, PyObject>,
    key: &str,
) -> Option<Origin> {
    let hash_key = String::from(key);
    let option: Option<Origin> = match map.get(&hash_key) {
        Some(value) => match value.extract::<String>(py).unwrap().parse() {
            Ok(origin) => Some(origin),
            _ => None,
        },
        _ => None,
    };
    option
}

pub fn extract_to_option_palette(
    py: Python<'_>,
    map: &HashMap<String, PyObject>,
    key: &str,
) -> Option<Palette> {
    let hash_key = String::from(key);
    let option: Option<Palette> = match map.get(&hash_key) {
        Some(value) => match value.extract::<String>(py).unwrap().parse() {
            Ok(palette) => Some(palette),
            _ => None,
        },
        _ => None,
    };
    option
}

pub fn extract_to_option_rounding_strategy(
    py: Python<'_>,
    map: &HashMap<String, PyObject>,
    key: &str,
) -> Option<RoundingStrategyWrapper> {
    let hash_key = String::from(key);
    let option: Option<RoundingStrategyWrapper> = match map.get(&hash_key) {
        Some(value) => value.extract::<RoundingStrategyWrapper>(py).ok(),
        _ => None,
    };
    option
}
