use indicatif::{ProgressBar, ProgressStyle};

use rust_decimal::prelude::*;

use crate::{cli::RoundingStrategyWrapper, plot::axis::Scale};

pub mod compact_float {
    //! rounds a float to 3 decimal places, when serialized into a str, such as for JSON
    //! offers space savings when such such precision is not needed.
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(float: &f64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{:.3}", float);
        let parsed = s.parse::<f64>().unwrap();
        serializer.serialize_f64(parsed)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<f64, D::Error>
    where
        D: Deserializer<'de>,
    {
        f64::deserialize(deserializer)
    }
}

/// # Examples
///
/// ```
/// # use crate::blobtk::utils::format_si;
/// assert_eq!(format_si(&123456.0, 1, None), "100k");
/// assert_eq!(format_si(&123456.0, 2, None), "120k");
/// assert_eq!(format_si(&123456.0, 3, None), "123k");
/// assert_eq!(format_si(&123456.0, 4, None), "123.5k");
/// assert_eq!(format_si(&12.3, 3, None), "12.3");
/// assert_eq!(format_si(&12.3, 2, None), "12");
/// assert_eq!(format_si(&0.02655, 3, None), "0.03");
/// assert_eq!(format_si(&0.021, 3, None), "0.02");
/// assert_eq!(format_si(&0.02, 1, None), "0");
/// assert_eq!(format_si(&0.0002, 3, None), "200μ");
/// assert_eq!(format_si(&0.000246, 2, None), "250μ");
/// assert_eq!(format_si(&0.00000246, 2, None), "2.5μ");
/// ```
pub fn format_si(value: &f64, digits: u32, rounding: Option<RoundingStrategyWrapper>) -> String {
    fn set_suffix(thousands: i8) -> String {
        const POSITIVE: [&str; 9] = ["", "k", "M", "G", "T", "P", "E", "Z", "Y"];
        const NEGATIVE: [&str; 8] = ["", "μ", "p", "n", "f", "a", "z", "y"];
        let suffix = if thousands < 0 && thousands >= -8 {
            NEGATIVE[(thousands * -1) as usize]
        } else if thousands >= 0 && thousands <= 9 {
            POSITIVE[thousands as usize]
        } else {
            ""
        };
        suffix.to_string()
    }

    let magnitude = if *value == 0.0 {
        0
    } else {
        (value.clone()).log10() as i8
    };
    let prefix;
    let thousands = magnitude / 3;
    if thousands < 0 {
        prefix = value.clone() * 10u32.pow(3 * (thousands.abs() as u32 + 1)) as f64;
    } else {
        prefix = value.clone() / 10u32.pow(3 * thousands as u32) as f64
    };
    let d = Decimal::from_f64_retain(prefix).unwrap();
    let rounding_strategy = match rounding {
        Some(RoundingStrategyWrapper::MidpointAwayFromZero) => {
            RoundingStrategy::MidpointAwayFromZero
        }
        Some(RoundingStrategyWrapper::ToZero) => RoundingStrategy::ToZero,
        Some(RoundingStrategyWrapper::AwayFromZero) => RoundingStrategy::AwayFromZero,
        _ => RoundingStrategy::MidpointAwayFromZero,
    };
    let mut rounded = d
        .round_sf_with_strategy(digits, rounding_strategy)
        .unwrap()
        .normalize()
        .to_string();
    if thousands == 0 && rounded.starts_with("0.") {
        let rounded_value = (rounded.parse::<f64>().unwrap() * 1000.0).round() / 1000.0;
        let digits_usize = &(digits as usize - 1);
        rounded = format!("{:.digits_usize$}", rounded_value);
    }

    let suffix = set_suffix(thousands);
    format!("{}{}", rounded, suffix)
}

pub fn format_pct(value: &f64, digits: u32, rounding: Option<RoundingStrategyWrapper>) -> String {
    let mut rounded = Decimal::from_f64_retain(*value).unwrap();
    let rounding_strategy = match rounding {
        Some(RoundingStrategyWrapper::MidpointAwayFromZero) => {
            RoundingStrategy::MidpointAwayFromZero
        }
        Some(RoundingStrategyWrapper::ToZero) => RoundingStrategy::ToZero,
        Some(RoundingStrategyWrapper::AwayFromZero) => RoundingStrategy::AwayFromZero,
        _ => RoundingStrategy::MidpointAwayFromZero,
    };
    rounded = rounded
        .round_dp_with_strategy(digits, rounding_strategy)
        .normalize();
    format!("{}%", rounded)
}

pub fn indexed_sort<T: Ord>(list: &[T]) -> Vec<usize> {
    let mut indices = (0..list.len()).collect::<Vec<_>>();
    indices.sort_by_key(|&i| &list[i]);
    indices.reverse();
    indices
}

pub fn styled_progress_bar(total: usize, message: &str) -> ProgressBar {
    let progress_bar = ProgressBar::new(total as u64);
    let format_string = format!(
        "[+]\t{}: {{bar:40.cyan/blue}} {{pos:>7}}/{{len:12}}",
        message
    );

    let pb_style_result = ProgressStyle::with_template(format_string.as_str());
    let pb_style = match pb_style_result {
        Ok(style) => style,
        Err(error) => panic!("Problem with the progress bar: {:?}", error),
    };
    progress_bar.set_style(pb_style);
    progress_bar
}

/// Scale a usize value from input domain to output range as f64.
/// # Examples
///
/// ```
/// # use crate::blobtk::utils::linear_scale;
/// let domain = [10, 20];
/// let range = [0.0, 100.0];
/// assert_eq!(linear_scale(15, &domain, &range), 50.0);
/// ```
pub fn linear_scale(value: usize, domain: &[usize; 2], range: &[f64; 2]) -> f64 {
    let proportion = (value - domain[0]) as f64 / (domain[1] - domain[0]) as f64;
    (range[1] - range[0]) * proportion + range[0]
}

/// Scale a f64 value from input domain to output range as f64.
/// # Examples
///
/// ```
/// # use crate::blobtk::utils::linear_scale_float;
/// let domain = [10.0, 20.0];
/// let range = [0.0, 100.0];
/// assert_eq!(linear_scale_float(15.0, &domain, &range), 50.0);
/// ```
pub fn linear_scale_float(value: f64, domain: &[f64; 2], range: &[f64; 2]) -> f64 {
    let proportion = (value - domain[0]) / (domain[1] - domain[0]);
    (range[1] - range[0]) * proportion + range[0]
}

pub fn log_scale(value: usize, domain: &[usize; 2], range: &[f64; 2]) -> f64 {
    let proportion = ((value as f64).log10() - (domain[0] as f64).log10())
        / ((domain[1] as f64).log10() - (domain[0] as f64).log10());
    (range[1] - range[0]) * proportion + range[0]
}

pub fn log_scale_float(value: f64, domain: &[f64; 2], range: &[f64; 2]) -> f64 {
    let proportion = (value.log10() - domain[0].log10()) / (domain[1].log10() - domain[0].log10());
    (range[1] - range[0]) * proportion + range[0]
}

/// # Examples
///
/// ```
/// # use crate::blobtk::utils::sqrt_scale;
/// let domain = [1, 25];
/// let range = [0.0, 100.0];
/// assert_eq!(sqrt_scale(1, &domain, &range), 0.0);
/// assert_eq!(sqrt_scale(4, &domain, &range), 25.0);
/// assert_eq!(sqrt_scale(9, &domain, &range), 50.0);
/// assert_eq!(sqrt_scale(16, &domain, &range), 75.0);
/// assert_eq!(sqrt_scale(25, &domain, &range), 100.0);
/// ```

pub fn sqrt_scale(value: usize, domain: &[usize; 2], range: &[f64; 2]) -> f64 {
    let proportion = ((value as f64).sqrt() - (domain[0] as f64).sqrt())
        / ((domain[1] as f64).sqrt() - (domain[0] as f64).sqrt());
    (range[1] - range[0]) * proportion + range[0]
}

/// # Examples
///
/// ```
/// # use crate::blobtk::utils::sqrt_scale_float;
/// let domain = [1.0, 25.0];
/// let range = [0.0, 100.0];
/// assert_eq!(sqrt_scale_float(1.0, &domain, &range), 0.0);
/// assert_eq!(sqrt_scale_float(4.0, &domain, &range), 25.0);
/// assert_eq!(sqrt_scale_float(9.0, &domain, &range), 50.0);
/// assert_eq!(sqrt_scale_float(16.0, &domain, &range), 75.0);
/// assert_eq!(sqrt_scale_float(25.0, &domain, &range), 100.0);
/// ```
pub fn sqrt_scale_float(value: f64, domain: &[f64; 2], range: &[f64; 2]) -> f64 {
    let proportion = (value.sqrt() - domain[0].sqrt()) / (domain[1].sqrt() - domain[0].sqrt());
    (range[1] - range[0]) * proportion + range[0]
}

pub fn scale_float(
    value: f64,
    domain: &[f64; 2],
    range: &[f64; 2],
    scale_type: &String,
    clamp: Option<f64>,
) -> f64 {
    let scale_log = &String::from("scaleLog");
    let scale_sqrt = &String::from("scaleSqrt");
    if clamp.is_some() {
        let clamp_value = clamp.unwrap();
        if value < clamp_value {
            return clamp_value;
        }
    }
    if scale_type == scale_log {
        log_scale_float(value, domain, range)
    } else if scale_type == scale_sqrt {
        sqrt_scale_float(value, domain, range)
    } else {
        linear_scale_float(value, domain, range)
    }
}

pub fn scale_floats(
    value: f64,
    domain: &[f64; 2],
    range: &[f64; 2],
    scale_type: &Scale,
    clamp: Option<f64>,
) -> f64 {
    if clamp.is_some() {
        let clamp_value = clamp.unwrap();
        if value < clamp_value {
            return match scale_type {
                Scale::LINEAR => linear_scale_float(clamp_value, domain, range),
                Scale::SQRT => sqrt_scale_float(clamp_value, domain, range),
                Scale::LOG => log_scale_float(clamp_value, domain, range),
            };
        }
    }
    match scale_type {
        Scale::LINEAR => linear_scale_float(value, domain, range),
        Scale::SQRT => sqrt_scale_float(value, domain, range),
        Scale::LOG => log_scale_float(value, domain, range),
    }
}

pub fn max_float<T: PartialOrd>(a: T, b: T) -> T {
    if b > a {
        b
    } else {
        a
    }
}

pub fn min_float<T: PartialOrd>(a: T, b: T) -> T {
    if a < b {
        a
    } else {
        b
    }
}
