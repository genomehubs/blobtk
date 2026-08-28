use serde_json::{Map, Value};

use crate::attribute_registry::AttributeRegistry;
use crate::index::es::models::{
    documents::AttributeDocument, nested_documents::NestedAttribute, IndexGroup,
};
use crate::parse::genomehubs::StringOrVec;
use crate::validation::spec::FieldType;

#[derive(Clone, Debug, Default)]
pub struct AttributeDocOverrides {
    pub display_name: Option<String>,
    pub display_group: Option<String>,
    pub display_level: Option<u8>,
    pub description: Option<String>,
    pub constraint: Option<Value>,
    pub field_type: Option<FieldType>,
    pub deprecated: Option<bool>,
    pub deprecated_reason: Option<String>,
}

pub fn feature_attribute_overrides(attr: &NestedAttribute) -> AttributeDocOverrides {
    let core_keys: &[&str] = &[
        "feature_id",
        "assembly_id",
        "taxon_id",
        "sequence_id",
        "start",
        "end",
        "strand",
        "length",
        "feature_name",
        "feature_type",
    ];
    let extended_keys: &[&str] = &[
        "seq_proportion",
        "midpoint",
        "midpoint_proportion",
        "chromosome_name",
    ];

    let display_group = if core_keys.contains(&attr.key.as_str()) {
        Some("core".to_string())
    } else if extended_keys.contains(&attr.key.as_str()) {
        Some("core".to_string())
    } else if attr.key.contains("_odb") {
        Some("busco".to_string())
    } else if attr.key.ends_with("_count") {
        Some("counts".to_string())
    } else if attr.key.contains("group_id")
        || attr.key.contains("group_set")
        || attr.key.contains("block_id")
        || attr.key.contains("locus")
        || attr.key.contains("synteny")
    {
        Some("synteny".to_string())
    } else {
        Some("stats".to_string())
    };

    let display_level =
        if core_keys.contains(&attr.key.as_str()) || extended_keys.contains(&attr.key.as_str()) {
            Some(1)
        } else {
            None
        };

    AttributeDocOverrides {
        display_group,
        display_level,
        ..Default::default()
    }
}

fn apply_registry_metadata(doc: &mut AttributeDocument, key: &str) {
    let Ok(registry) = AttributeRegistry::load_default() else {
        return;
    };
    let Some(entry) = registry.lookup(key) else {
        return;
    };

    if doc.display_name.is_none() || doc.display_name == Some(key.to_string()) {
        doc.display_name = entry.display_name.clone().or_else(|| Some(key.to_string()));
    }
    if doc.display_group.is_none() {
        doc.display_group = entry.display_group.clone();
    }
    if doc.display_level.is_none() {
        doc.display_level = entry.display_level;
    }
    if doc.description.is_none() {
        doc.description = entry.description.clone();
    }
    if doc.field_type == FieldType::Keyword && entry.r#type.is_some() {
        match entry.r#type.as_deref() {
            Some("long") => doc.field_type = FieldType::Long,
            Some("integer") => doc.field_type = FieldType::Integer,
            Some("float") => doc.field_type = FieldType::Float,
            Some("double") => doc.field_type = FieldType::Double,
            Some("byte") => doc.field_type = FieldType::Byte,
            Some("short") => doc.field_type = FieldType::Short,
            Some("date") => doc.field_type = FieldType::Date,
            Some("boolean") => doc.field_type = FieldType::Boolean,
            Some("half_float") => doc.field_type = FieldType::HalfFloat,
            Some("1dp") => doc.field_type = FieldType::OneDP,
            Some("2dp") => doc.field_type = FieldType::TwoDP,
            Some("3dp") => doc.field_type = FieldType::ThreeDP,
            Some("4dp") => doc.field_type = FieldType::FourDP,
            _ => {}
        }
    }
}

pub fn build_attribute_document(
    attr: &NestedAttribute,
    overrides: Option<&AttributeDocOverrides>,
) -> AttributeDocument {
    let mut doc = AttributeDocument {
        group: IndexGroup::Feature,
        name: attr.key.clone(),
        display_name: Some(attr.key.clone()),
        description: None,
        field_type: infer_field_type(attr),
        constraint: infer_constraint(attr),
        ..Default::default()
    };

    apply_registry_metadata(&mut doc, &attr.key);

    if let Some(overrides) = overrides {
        if let Some(display_name) = &overrides.display_name {
            doc.display_name = Some(display_name.clone());
        }
        if let Some(display_group) = &overrides.display_group {
            doc.display_group = Some(display_group.clone());
        }
        if let Some(display_level) = &overrides.display_level {
            doc.display_level = Some(*display_level);
        }
        if let Some(description) = &overrides.description {
            doc.description = Some(description.clone());
        }
        if let Some(field_type) = &overrides.field_type {
            doc.field_type = field_type.clone();
        }
        if let Some(deprecated) = overrides.deprecated {
            doc.deprecated = Some(deprecated);
        }
        if let Some(deprecated_reason) = &overrides.deprecated_reason {
            doc.deprecated_reason = Some(deprecated_reason.clone());
        }
        doc.constraint = merge_constraints(doc.constraint.clone(), overrides.constraint.clone());
    }

    doc
}

pub fn merge_attribute_documents(
    existing: &AttributeDocument,
    candidate: &AttributeDocument,
) -> AttributeDocument {
    let mut merged = existing.clone();

    if merged.display_name.is_none() {
        merged.display_name = candidate.display_name.clone();
    }
    if merged.display_group.is_none() {
        merged.display_group = candidate.display_group.clone();
    }
    if merged.display_level.is_none() {
        merged.display_level = candidate.display_level.clone();
    }
    if merged.description.is_none() {
        merged.description = candidate.description.clone();
    }
    if merged.deprecated.is_none() {
        merged.deprecated = candidate.deprecated;
    }
    if merged.deprecated_reason.is_none() {
        merged.deprecated_reason = candidate.deprecated_reason.clone();
    }
    if merged.units.is_none() {
        merged.units = candidate.units.clone();
    }
    if merged.value_metadata.is_none() {
        merged.value_metadata = candidate.value_metadata.clone();
    }

    if merged.field_type == FieldType::Keyword && candidate.field_type != FieldType::Keyword {
        merged.field_type = candidate.field_type.clone();
    }

    merged.constraint =
        merge_constraints(existing.constraint.clone(), candidate.constraint.clone());
    merged
}

fn infer_field_type(attr: &NestedAttribute) -> FieldType {
    if attr.bool_value.is_some() || attr.is_primary_value.is_some() || attr.deprecated.is_some() {
        FieldType::Boolean
    } else if attr.byte_value.is_some() {
        FieldType::Byte
    } else if attr.date_value.is_some() || attr.source_date.is_some() {
        FieldType::Date
    } else if attr.double_value.is_some() {
        FieldType::Double
    } else if attr.float_value.is_some() {
        FieldType::Float
    } else if attr.geo_point_value.is_some() {
        FieldType::GeoPoint
    } else if attr.half_float_value.is_some() {
        FieldType::HalfFloat
    } else if attr.integer_value.is_some() || attr.count.is_some() {
        FieldType::Integer
    } else if attr.long_value.is_some() {
        FieldType::Long
    } else if attr.short_value.is_some() || attr.source_year.is_some() {
        FieldType::Short
    } else if attr.one_dp_value.is_some() {
        FieldType::OneDP
    } else if attr.two_dp_value.is_some() {
        FieldType::TwoDP
    } else if attr.three_dp_value.is_some() {
        FieldType::ThreeDP
    } else if attr.four_dp_value.is_some() {
        FieldType::FourDP
    } else {
        FieldType::Keyword
    }
}

fn infer_constraint(attr: &NestedAttribute) -> Option<Value> {
    let mut constraint = Map::new();

    if attr.key == "status" || attr.key.ends_with("_status") {
        let values = keyword_values(attr);
        if !values.is_empty() {
            constraint.insert(
                "enum".to_string(),
                Value::Array(values.into_iter().map(Value::String).collect()),
            );
        }
    }

    if attr.key.contains("proportion") {
        constraint.insert("min".to_string(), Value::from(0));
        constraint.insert("max".to_string(), Value::from(1));
    }

    if attr.key.ends_with("_count") {
        constraint.insert("min".to_string(), Value::from(0));
    }

    if constraint.is_empty() {
        None
    } else {
        Some(Value::Object(constraint))
    }
}

fn keyword_values(attr: &NestedAttribute) -> Vec<String> {
    match &attr.keyword_value {
        Some(StringOrVec::Single(value)) => vec![value.to_lowercase()],
        Some(StringOrVec::Multiple(values)) => {
            let mut lowered: Vec<String> =
                values.iter().map(|value| value.to_lowercase()).collect();
            lowered.sort();
            lowered.dedup();
            lowered
        }
        None => Vec::new(),
    }
}

pub fn merge_constraints(existing: Option<Value>, candidate: Option<Value>) -> Option<Value> {
    match (existing, candidate) {
        (None, None) => None,
        (Some(existing), None) => Some(existing),
        (None, Some(candidate)) => Some(candidate),
        (Some(Value::Object(mut existing_map)), Some(Value::Object(candidate_map))) => {
            for (key, candidate_value) in candidate_map {
                match (
                    key.as_str(),
                    existing_map.get(&key).cloned(),
                    candidate_value,
                ) {
                    (
                        "enum",
                        Some(Value::Array(existing_values)),
                        Value::Array(candidate_values),
                    ) => {
                        let mut merged_values: Vec<String> = existing_values
                            .into_iter()
                            .chain(candidate_values.into_iter())
                            .filter_map(|value| value.as_str().map(|value| value.to_lowercase()))
                            .collect();
                        merged_values.sort();
                        merged_values.dedup();
                        existing_map.insert(
                            key,
                            Value::Array(merged_values.into_iter().map(Value::String).collect()),
                        );
                    }
                    ("min", Some(Value::Number(existing_num)), Value::Number(candidate_num)) => {
                        let merged = match (existing_num.as_f64(), candidate_num.as_f64()) {
                            (Some(existing), Some(candidate)) => existing.min(candidate),
                            _ => continue,
                        };
                        existing_map.insert(key, Value::from(merged));
                    }
                    ("max", Some(Value::Number(existing_num)), Value::Number(candidate_num)) => {
                        let merged = match (existing_num.as_f64(), candidate_num.as_f64()) {
                            (Some(existing), Some(candidate)) => existing.max(candidate),
                            _ => continue,
                        };
                        existing_map.insert(key, Value::from(merged));
                    }
                    (_, None, value) => {
                        existing_map.insert(key, value);
                    }
                    _ => {}
                }
            }
            Some(Value::Object(existing_map))
        }
        (Some(existing), Some(_candidate)) => Some(existing),
    }
}
