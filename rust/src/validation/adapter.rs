use crate::error::Error;
use crate::parse::genomehubs::{GHubsConfig, GHubsFieldConfig};
use crate::validation::spec::{ConstraintConfig, FieldSpec};
use crate::validation::types::ValidationReport;
use crate::validation::validator::RowValidator;
use std::collections::HashMap;

pub struct GenomeHubsValidator<'a> {
    pub cfg: &'a mut GHubsConfig,
}

impl<'a> GenomeHubsValidator<'a> {
    pub fn new(cfg: &'a mut GHubsConfig) -> Self {
        Self { cfg }
    }

    pub fn to_field_spec(&self, f: &GHubsFieldConfig) -> FieldSpec {
        FieldSpec {
            field_type: f.field_type.clone(), // ensure FieldType is shared or mapped
            constraint: f.constraint.clone().or(Some(ConstraintConfig::default())),
        }
    }
}

impl<'a> RowValidator for GenomeHubsValidator<'a> {
    fn validate_row(&mut self, row: &HashMap<String, String>) -> Result<ValidationReport, Error> {
        let keys = vec!["attributes", "taxonomy", "taxon_names"];
        let (_processed, report) = self.cfg.validate_record_from_map(row, 0, &keys);
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::genomehubs::{GHubsConfig, GHubsFieldConfig};
    use crate::validation::spec::{ConstraintConfig, FieldType};
    use crate::validation::types::ValidationStatus;
    use std::collections::HashMap;

    #[test]
    fn genomehubs_validator_valid_row() {
        // Setup minimal GHubsConfig with one attribute "foo"
        let mut cfg = GHubsConfig::default();
        let mut attrs = HashMap::new();
        let field_cfg = GHubsFieldConfig {
            field_type: FieldType::Keyword,
            ..Default::default()
        };
        // leave default field_type (keyword) and no constraints
        attrs.insert("foo".to_string(), field_cfg);
        cfg.attributes = Some(attrs);

        let mut validator = GenomeHubsValidator::new(&mut cfg);
        let mut row = HashMap::new();
        row.insert("foo".to_string(), "somevalue".to_string());

        let report = validator.validate_row(&row).expect("validate_row failed");
        assert_eq!(report.status, ValidationStatus::Valid);
        assert_eq!(report.counts.valid, 1);
    }

    #[test]
    fn genomehubs_validator_invalid_by_constraint() {
        // Setup GHubsConfig with constraint enum ["allowed"]
        let mut cfg = GHubsConfig::default();
        let mut attrs = HashMap::new();
        let field_cfg = GHubsFieldConfig {
            field_type: FieldType::Keyword,
            constraint: Some(ConstraintConfig {
                enum_values: Some(vec!["allowed".to_string()]),
                ..Default::default()
            }),
            ..Default::default()
        };
        attrs.insert("col".to_string(), field_cfg);
        cfg.attributes = Some(attrs);

        let mut validator = GenomeHubsValidator::new(&mut cfg);
        let mut row = HashMap::new();
        row.insert("col".to_string(), "not_allowed".to_string());
        let report = validator.validate_row(&row).expect("validate_row failed");
        // single invalid field should lead to Invalid or Partial depending on implementation;
        // earlier logic sets Invalid when nothing valid, Partial when some valid — here expect Invalid
        assert!(matches!(
            report.status,
            ValidationStatus::Invalid | ValidationStatus::Partial
        ));
        assert!(report.counts.invalid >= 1);
    }
}
