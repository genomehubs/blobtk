use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

// Field types
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub enum FieldType {
    #[serde(rename = "byte")]
    Byte,
    #[serde(rename = "date")]
    Date,
    #[serde(rename = "double")]
    Double,
    #[serde(rename = "float")]
    Float,
    #[serde(rename = "geo_point")]
    GeoPoint,
    #[serde(rename = "half_float")]
    HalfFloat,
    #[default]
    #[serde(rename = "keyword")]
    Keyword,
    #[serde(rename = "integer")]
    Integer,
    #[serde(rename = "long")]
    Long,
    #[serde(rename = "short")]
    Short,
    #[serde(rename = "1dp")]
    OneDP,
    #[serde(rename = "2dp")]
    TwoDP,
    #[serde(rename = "3dp")]
    ThreeDP,
    #[serde(rename = "4dp")]
    FourDP,
}

pub fn default_field_type() -> FieldType {
    FieldType::Keyword
}

pub struct FieldSpec {
    pub field_type: FieldType,
    pub constraint: Option<ConstraintConfig>,
}

/// GenomeHubs field constraint configuration options
#[derive(Default, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConstraintConfig {
    // List of valid values
    #[serde(
        rename = "enum",
        deserialize_with = "deserialize_to_lowercase",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub enum_values: Option<Vec<String>>,
    // Value length
    #[serde(rename = "len", skip_serializing_if = "Option::is_none")]
    pub len: Option<usize>,
    // Maximum value
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "format_number"
    )]
    pub max: Option<f64>,
    // Minimum value
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "format_number"
    )]
    pub min: Option<f64>,
}

fn deserialize_to_lowercase<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let v: Vec<String> = Vec::deserialize(deserializer)?;
    Ok(Some(v.into_iter().map(|s| s.to_lowercase()).collect()))
}

// format numbers such that any float ending in .0 is converted to an integer
fn format_number<S>(number: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if let Some(number) = number {
        if number.fract() == 0.0 {
            serializer.serialize_i64(number.trunc() as i64)
        } else {
            serializer.serialize_f64(*number)
        }
    } else {
        serializer.serialize_none()
    }
}
