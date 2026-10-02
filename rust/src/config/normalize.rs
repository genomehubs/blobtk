use std::collections::HashMap;

use crate::config::schema::{
    AssemblyPolicyConfig, IndexingConfig, ScaffoldPolicyConfig, StagedImportConfig,
};

/// Normalize the staged config into a consistent runtime shape before validation.
///
/// This lives in the config subsystem rather than the legacy compatibility layer,
/// so the runtime can evolve without a large ad hoc schema surface.
pub fn normalize_staged_import_config(staged: &mut StagedImportConfig) {
    if staged.sequence.metadata.is_empty() {
        staged.sequence.metadata = HashMap::new();
    }

    if staged.annotations.is_empty() {
        staged.annotations = HashMap::new();
    }

    for annotation in staged.annotations.values_mut() {
        if annotation.assign_to.is_empty() {
            annotation.assign_to = vec!["sequence".to_string()];
        }
    }

    if staged.windowing.bed_resolution.is_none() {
        staged.windowing.bed_resolution = Some(staged.windowing.lines_per_unit.max(1));
    }
    // lines_per_unit is the field actually threaded through to the runtime bed
    // parser as the window step size, so it must always agree with bed_resolution
    // even when only bed_resolution was set in the config.
    staged.windowing.lines_per_unit = staged.windowing.bed_resolution.unwrap_or(1).max(1);

    for spec in &mut staged.windowing.windows {
        match spec {
            crate::parse::bed::WindowSpec::Size {
                remnant_bounds,
                remnant_policy,
                ..
            } => {
                if remnant_bounds.is_none() {
                    *remnant_bounds = Some(crate::parse::bed::RemnantBoundsConfig {
                        min_fraction: 0.67,
                        max_fraction: 1.33,
                    });
                }
                if *remnant_policy == crate::parse::bed::RemnantPolicy::Trailing {
                    *remnant_policy = crate::parse::bed::RemnantPolicy::Centered;
                }
            }
            crate::parse::bed::WindowSpec::Proportion {
                min_size,
                proportion,
                ..
            } => {
                if min_size.is_none() && *proportion > 0.0 {
                    *min_size = Some(((*proportion * 1_000_000.0).ceil() as usize).max(1));
                }
            }
        }
    }

    if staged.assembly_policy.is_none() {
        staged.assembly_policy = Some(AssemblyPolicyConfig {
            min_chromosome_fraction: 0.9,
            fallback_mode: "minimal".to_string(),
        });
    }

    if staged.scaffold_policy.is_none() {
        staged.scaffold_policy = Some(ScaffoldPolicyConfig {
            min_scaffold_length: 1_000_000,
            skip_short_scaffolds_without_data: true,
            index_small_scaffold_as_parent_if: vec!["busco".to_string()],
        });
    }

    if staged.indexing.is_none() {
        staged.indexing = Some(IndexingConfig {
            profile: "standard".to_string(),
        });
    }
}

#[deprecated(
    note = "compatibility shim for legacy config; prefer config::normalize::normalize_staged_import_config"
)]
pub fn normalize_legacy_staged_config(staged: &mut StagedImportConfig) {
    normalize_staged_import_config(staged);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::AnnotationSourceConfig;
    use crate::config::schema::ResolvedPathConfig;

    #[test]
    fn adds_default_assignment_for_unassigned_annotation() {
        let mut staged = StagedImportConfig {
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
            sequence: crate::config::schema::SequenceMetadataConfig {
                report: ResolvedPathConfig {
                    path: Some(std::path::PathBuf::from("/tmp/report.jsonl")),
                    local_path: None,
                },
                metadata: HashMap::new(),
            },
            annotations: HashMap::from_iter([(
                "custom".to_string(),
                AnnotationSourceConfig {
                    source: ResolvedPathConfig {
                        path: Some(std::path::PathBuf::from("/tmp/custom.bed.gz")),
                        local_path: None,
                    },
                    assign_to: vec![],
                    fields: HashMap::new(),
                    algs: None,
                },
            )]),
            windowing: crate::config::schema::WindowingConfig {
                lines_per_unit: 1000,
                windows: vec![],
                files: vec![],
                target_size: None,
                bed_resolution: None,
                remnant_policy: None,
                remnant_bounds: None,
            },
            assembly_policy: None,
            scaffold_policy: None,
            indexing: None,
            derived_metrics: vec![],
            import: None,
        };

        normalize_staged_import_config(&mut staged);
        assert_eq!(staged.annotations["custom"].assign_to, vec!["sequence"]);
        assert_eq!(staged.windowing.bed_resolution, Some(1000));
        assert!(staged.assembly_policy.is_some());
        assert!(staged.scaffold_policy.is_some());
        assert!(staged.indexing.is_some());
    }

    #[test]
    fn syncs_lines_per_unit_from_bed_resolution_when_only_bed_resolution_is_set() {
        let mut staged = StagedImportConfig {
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
            sequence: crate::config::schema::SequenceMetadataConfig {
                report: ResolvedPathConfig {
                    path: Some(std::path::PathBuf::from("/tmp/report.jsonl")),
                    local_path: None,
                },
                metadata: HashMap::new(),
            },
            annotations: HashMap::new(),
            windowing: crate::config::schema::WindowingConfig {
                lines_per_unit: 0,
                windows: vec![],
                files: vec![],
                target_size: None,
                bed_resolution: Some(1000),
                remnant_policy: None,
                remnant_bounds: None,
            },
            assembly_policy: None,
            scaffold_policy: None,
            indexing: None,
            derived_metrics: vec![],
            import: None,
        };

        normalize_staged_import_config(&mut staged);
        assert_eq!(staged.windowing.bed_resolution, Some(1000));
        assert_eq!(staged.windowing.lines_per_unit, 1000);
    }
}
