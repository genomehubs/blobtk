use crate::parse::lookup::TaxonMatch;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ValidationStatus {
    Valid,
    Invalid,
    Partial,
    Blank,
    Error,
    #[default]
    None,
    Spellcheck,
    Putative,
    Mismatch,
    Multimatch,
    Nomatch,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ValidationCounts {
    pub total: usize,
    pub valid: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub invalid: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub partial: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub blank: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub errors: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub spellcheck: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub putative: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub mismatch: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub multimatch: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub nomatch: usize,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

impl ValidationCounts {
    pub fn to_json(&self) -> String {
        // summarise as json
        serde_json::to_string_pretty(&self).unwrap()
    }

    pub fn to_jsonl(&self) -> String {
        // summarise as jsonl
        serde_json::to_string(&self).unwrap()
    }

    pub fn update(&mut self, other: &ValidationCounts) {
        if other.total >= 1 {
            self.total += 1
        };
        if other.valid >= 1 {
            self.valid += 1
        };
        if other.invalid >= 1 {
            self.invalid += 1
        };
        if other.partial >= 1 {
            self.partial += 1
        };
        if other.blank >= 1 {
            self.blank += 1
        };
        if other.errors >= 1 {
            self.errors += 1
        };
        if other.spellcheck >= 1 {
            self.spellcheck += 1
        };
        if other.putative >= 1 {
            self.putative += 1
        };
        if other.mismatch >= 1 {
            self.mismatch += 1
        };
        if other.multimatch >= 1 {
            self.multimatch += 1
        };
        if other.nomatch >= 1 {
            self.nomatch += 1
        };
    }
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct ValidationReport {
    pub row_index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxon_name: Option<String>,
    pub status: ValidationStatus,
    pub counts: ValidationCounts,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub invalid: HashMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub partial: HashMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blank: Vec<String>,
    #[serde(skip_serializing)]
    pub validated: HashMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spellcheck: Vec<TaxonMatch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub putative: Vec<TaxonMatch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mismatch: Vec<TaxonMatch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub multimatch: Vec<TaxonMatch>,
}

impl ValidationReport {
    pub fn to_json(&self) -> String {
        // summarise as json
        serde_json::to_string_pretty(&self).unwrap()
    }

    pub fn to_jsonl(&self) -> String {
        // summarise as jsonl
        serde_json::to_string(&self).unwrap()
    }

    pub fn combine_reports(&mut self, other: ValidationReport) {
        self.status = match other.status {
            ValidationStatus::Partial => ValidationStatus::Partial,
            ValidationStatus::Error => ValidationStatus::Error,
            _ => {
                if self.status == other.status {
                    self.status.clone()
                } else if self.status == ValidationStatus::None {
                    other.status
                } else if self.status == ValidationStatus::Valid
                    && other.status == ValidationStatus::Invalid
                {
                    ValidationStatus::Partial
                } else if self.status == ValidationStatus::Invalid
                    && other.status == ValidationStatus::Valid
                {
                    ValidationStatus::Partial
                } else {
                    self.status.clone()
                }
            }
        };
        self.counts.valid += other.counts.valid;
        self.counts.invalid += other.counts.invalid;
        self.counts.partial += other.counts.partial;
        self.counts.blank += other.counts.blank;
        self.counts.errors += other.counts.errors;
        self.counts.total += other.counts.total;
        self.invalid.extend(other.invalid);
        self.partial.extend(other.partial);
        self.blank.extend(other.blank);
        self.validated.extend(other.validated);
    }
}
