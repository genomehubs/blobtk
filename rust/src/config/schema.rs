use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedPathConfig {
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub local_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationSourceConfig {
    pub source: ResolvedPathConfig,
    #[serde(default)]
    pub assign_to: Vec<String>,
    #[serde(default)]
    pub fields: HashMap<String, String>,
    #[serde(default)]
    pub algs: Option<Vec<crate::parse::busco::AlgConfig>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SequenceMetadataConfig {
    pub report: ResolvedPathConfig,
    #[serde(default)]
    pub metadata: HashMap<String, AnnotationSourceConfig>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RemnantBoundsConfig {
    pub min_fraction: f64,
    pub max_fraction: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WindowingConfig {
    #[serde(default, alias = "lines_per_unit")]
    pub lines_per_unit: usize,
    #[serde(default)]
    pub windows: Vec<crate::parse::bed::WindowSpec>,
    #[serde(default)]
    pub files: Vec<crate::parse::bed::BedConfig>,
    #[serde(default, alias = "target_size")]
    pub target_size: Option<usize>,
    #[serde(default)]
    pub bed_resolution: Option<usize>,
    #[serde(default, alias = "remnant_policy")]
    pub remnant_policy: Option<crate::parse::bed::RemnantPolicy>,
    #[serde(default, alias = "remnant_bounds")]
    pub remnant_bounds: Option<RemnantBoundsConfig>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AssemblyPolicyConfig {
    pub min_chromosome_fraction: f64,
    pub fallback_mode: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ScaffoldPolicyConfig {
    pub min_scaffold_length: usize,
    pub skip_short_scaffolds_without_data: bool,
    #[serde(default)]
    pub index_small_scaffold_as_parent_if: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct IndexingConfig {
    pub profile: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DerivedMetricConfig {
    pub name: String,
    pub target: String,
    pub source: String,
    #[serde(default)]
    pub anchor: Option<String>,
    #[serde(default)]
    pub output_type: Option<String>,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StagedImportConfig {
    pub assembly: crate::import::AssemblyImportConfig,
    pub es: crate::import::EsConfig,
    pub sequence: SequenceMetadataConfig,
    #[serde(default)]
    pub annotations: HashMap<String, AnnotationSourceConfig>,
    #[serde(default)]
    pub windowing: WindowingConfig,
    #[serde(default)]
    pub assembly_policy: Option<AssemblyPolicyConfig>,
    #[serde(default)]
    pub scaffold_policy: Option<ScaffoldPolicyConfig>,
    #[serde(default)]
    pub indexing: Option<IndexingConfig>,
    #[serde(default)]
    pub derived_metrics: Vec<DerivedMetricConfig>,
    #[serde(default)]
    pub import: Option<crate::import::ImportOptions>,
}
