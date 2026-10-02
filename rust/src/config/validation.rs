use crate::config::paths::resolve_source_path;
use crate::config::schema::{ResolvedPathConfig, StagedImportConfig};

pub fn validate_staged_import_config(staged: &StagedImportConfig) -> Result<(), anyhow::Error> {
    if staged.sequence.report.path.is_none() && staged.sequence.report.local_path.is_none() {
        return Err(anyhow::anyhow!(
            "staged import config is missing a sequence report path or local_path"
        ));
    }
    resolve_source_path(&staged.sequence.report)?;

    for file in &staged.windowing.files {
        let resolved = resolve_source_path(&ResolvedPathConfig {
            path: Some(file.path.clone()),
            local_path: file.local_path.clone(),
        })?;
        let _ = resolved;
    }

    for annotation in staged.annotations.values() {
        resolve_source_path(&annotation.source)?;
    }

    if staged.windowing.files.is_empty() {
        return Err(anyhow::anyhow!(
            "staged import config is missing any window bed files"
        ));
    }

    let bed_resolution = staged
        .windowing
        .bed_resolution
        .unwrap_or(staged.windowing.lines_per_unit.max(1));
    if bed_resolution == 0 {
        return Err(anyhow::anyhow!("windowing.bed_resolution must be greater than zero"));
    }

    for window_spec in &staged.windowing.windows {
        match window_spec {
            crate::parse::bed::WindowSpec::Size {
                size,
                remnant_policy: _,
                remnant_bounds,
            } => {
                if *size <= bed_resolution {
                    return Err(anyhow::anyhow!(
                        "window size ({size}) must be greater than bed_resolution ({bed_resolution})"
                    ));
                }
                if let Some(bounds) = remnant_bounds {
                    if bounds.min_fraction <= 0.0 || bounds.max_fraction <= 0.0 {
                        return Err(anyhow::anyhow!(
                            "size window remnant_bounds min_fraction and max_fraction must be greater than zero"
                        ));
                    }
                    if bounds.min_fraction > 1.0 {
                        return Err(anyhow::anyhow!(
                            "size window remnant_bounds.min_fraction must be <= 1.0"
                        ));
                    }
                    if bounds.max_fraction < 1.0 {
                        return Err(anyhow::anyhow!(
                            "size window remnant_bounds.max_fraction must be >= 1.0"
                        ));
                    }
                    if bounds.min_fraction >= bounds.max_fraction {
                        return Err(anyhow::anyhow!(
                            "size window remnant_bounds.min_fraction must be less than max_fraction"
                        ));
                    }
                }
            }
            crate::parse::bed::WindowSpec::Proportion { proportion, min_size } => {
                if *proportion <= 0.0 {
                    return Err(anyhow::anyhow!(
                        "proportion window proportion must be greater than zero"
                    ));
                }
                if let Some(min_size) = min_size {
                    if *min_size <= bed_resolution {
                        return Err(anyhow::anyhow!(
                            "proportion window min_size ({min_size}) must be greater than bed_resolution ({bed_resolution})"
                        ));
                    }
                }
            }
        }
    }

    if let Some(policy) = &staged.assembly_policy {
        if !(0.0..=1.0).contains(&policy.min_chromosome_fraction) {
            return Err(anyhow::anyhow!(
                "assembly_policy.min_chromosome_fraction must be within [0, 1]"
            ));
        }
        match policy.fallback_mode.as_str() {
            "standard" | "minimal" | "skip_windows" | "abort" => {}
            _ => {
                return Err(anyhow::anyhow!(
                    "assembly_policy.fallback_mode must be one of: standard, minimal, skip_windows, abort"
                ));
            }
        }
    }

    if let Some(policy) = &staged.scaffold_policy {
        if policy.min_scaffold_length == 0 {
            return Err(anyhow::anyhow!(
                "scaffold_policy.min_scaffold_length must be greater than zero"
            ));
        }
        for value in &policy.index_small_scaffold_as_parent_if {
            if !matches!(value.as_str(), "busco" | "annotation" | "synteny") {
                return Err(anyhow::anyhow!(
                    "scaffold_policy.index_small_scaffold_as_parent_if contains unsupported value '{}'; allowed: busco, annotation, synteny",
                    value
                ));
            }
        }
    }

    if let Some(policy) = &staged.indexing {
        match policy.profile.as_str() {
            "standard" | "minimal" | "chromosome_only" | "feature_only" => {}
            _ => {
                return Err(anyhow::anyhow!(
                    "indexing.profile must be one of: standard, minimal, chromosome_only, feature_only"
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{
        AnnotationSourceConfig, ResolvedPathConfig, SequenceMetadataConfig,
    };
    use std::collections::HashMap;

    #[test]
    fn rejects_missing_sequence_report() {
        let staged = StagedImportConfig {
            assembly: crate::import::AssemblyImportConfig {
                accession: "GCA_00000001.1".to_string(),
                taxon_id: None,
                ancestors: vec![],
                lineage: vec![],
            },
            es: crate::import::EsConfig {
                host: "http://localhost".to_string(),
                port: 9200,
                username: None,
                password: None,
                hub: crate::import::HubConfig {
                    name: "goat".to_string(),
                    release: "test".to_string(),
                    taxonomy: "ncbi".to_string(),
                },
            },
            sequence: SequenceMetadataConfig {
                report: ResolvedPathConfig {
                    path: None,
                    local_path: None,
                },
                metadata: HashMap::new(),
            },
            annotations: HashMap::from_iter([(
                "annotation".to_string(),
                AnnotationSourceConfig {
                    source: ResolvedPathConfig {
                        path: Some(std::path::PathBuf::from("/tmp/annotation.bed.gz")),
                        local_path: None,
                    },
                    assign_to: vec!["window".to_string()],
                    fields: HashMap::new(),
                    algs: None,
                },
            )]),
            windowing: crate::config::schema::WindowingConfig {
                lines_per_unit: 1000,
                windows: vec![],
                files: vec![crate::parse::bed::BedConfig {
                    path: std::path::PathBuf::from("/tmp/window.bed.gz"),
                    local_path: None,
                    value_columns: vec![],
                }],
                target_size: None,
                bed_resolution: None,
                remnant_policy: None,
                remnant_bounds: None,
            },
            assembly_policy: Some(crate::config::schema::AssemblyPolicyConfig {
                min_chromosome_fraction: 0.9,
                fallback_mode: "minimal".to_string(),
            }),
            scaffold_policy: Some(crate::config::schema::ScaffoldPolicyConfig {
                min_scaffold_length: 1_000_000,
                skip_short_scaffolds_without_data: true,
                index_small_scaffold_as_parent_if: vec!["busco".to_string()],
            }),
            indexing: Some(crate::config::schema::IndexingConfig {
                profile: "standard".to_string(),
            }),
            derived_metrics: vec![],
            import: None,
        };

        assert!(validate_staged_import_config(&staged).is_err());
    }

    #[test]
    fn rejects_invalid_window_policy_bounds() {
        let staged = StagedImportConfig {
            assembly: crate::import::AssemblyImportConfig {
                accession: "GCA_00000001.1".to_string(),
                taxon_id: None,
                ancestors: vec![],
                lineage: vec![],
            },
            es: crate::import::EsConfig {
                host: "http://localhost".to_string(),
                port: 9200,
                username: None,
                password: None,
                hub: crate::import::HubConfig {
                    name: "goat".to_string(),
                    release: "test".to_string(),
                    taxonomy: "ncbi".to_string(),
                },
            },
            sequence: SequenceMetadataConfig {
                report: ResolvedPathConfig {
                    path: Some(std::path::PathBuf::from("/tmp/report.jsonl")),
                    local_path: None,
                },
                metadata: HashMap::new(),
            },
            annotations: HashMap::new(),
            windowing: crate::config::schema::WindowingConfig {
                lines_per_unit: 1000,
                windows: vec![],
                files: vec![crate::parse::bed::BedConfig {
                    path: std::path::PathBuf::from("/tmp/window.bed.gz"),
                    local_path: None,
                    value_columns: vec![],
                }],
                target_size: Some(1_000_000),
                bed_resolution: Some(1_000),
                remnant_policy: Some(crate::parse::bed::RemnantPolicy::Centered),
                remnant_bounds: Some(crate::config::schema::RemnantBoundsConfig {
                    min_fraction: 1.5,
                    max_fraction: 1.0,
                }),
            },
            assembly_policy: Some(crate::config::schema::AssemblyPolicyConfig {
                min_chromosome_fraction: 0.9,
                fallback_mode: "minimal".to_string(),
            }),
            scaffold_policy: Some(crate::config::schema::ScaffoldPolicyConfig {
                min_scaffold_length: 1_000_000,
                skip_short_scaffolds_without_data: true,
                index_small_scaffold_as_parent_if: vec!["busco".to_string()],
            }),
            indexing: Some(crate::config::schema::IndexingConfig {
                profile: "standard".to_string(),
            }),
            derived_metrics: vec![],
            import: None,
        };

        assert!(validate_staged_import_config(&staged).is_err());
    }
}
