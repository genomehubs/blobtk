use std::collections::HashMap;

use crate::config::normalize::normalize_staged_import_config;
use crate::config::schema::{
    AnnotationSourceConfig, AssemblyPolicyConfig, DerivedMetricConfig, IndexingConfig,
    ResolvedPathConfig, ScaffoldPolicyConfig, SequenceMetadataConfig, StagedImportConfig,
    WindowingConfig,
};
use crate::import::{ImportConfig, SequenceReportImportConfig};

fn annotation_source_from_path(
    path: Option<std::path::PathBuf>,
    local_path: Option<std::path::PathBuf>,
) -> ResolvedPathConfig {
    ResolvedPathConfig { path, local_path }
}

pub fn expand_staged_placeholders(staged: &mut StagedImportConfig) {
    let accession = staged.assembly.accession.clone();
    let taxon = staged.assembly.taxon_id.clone().unwrap_or_default();

    let replace_tokens = |s: String| {
        s.replace("{ACCESSION}", &accession)
            .replace("{TAXON}", &taxon)
            .replace("{TAXON_ID}", &taxon)
    };

    if let Some(path) = staged.sequence.report.path.as_ref() {
        staged.sequence.report.path = Some(std::path::PathBuf::from(replace_tokens(
            path.to_string_lossy().to_string(),
        )));
    }
    if let Some(local_path) = staged.sequence.report.local_path.as_ref() {
        staged.sequence.report.local_path = Some(std::path::PathBuf::from(replace_tokens(
            local_path.to_string_lossy().to_string(),
        )));
    }

    for annotation in staged.annotations.values_mut() {
        if let Some(path) = annotation.source.path.as_ref() {
            annotation.source.path = Some(std::path::PathBuf::from(replace_tokens(
                path.to_string_lossy().to_string(),
            )));
        }
        if let Some(local_path) = annotation.source.local_path.as_ref() {
            annotation.source.local_path = Some(std::path::PathBuf::from(replace_tokens(
                local_path.to_string_lossy().to_string(),
            )));
        }
    }

    for bed in staged.windowing.files.iter_mut() {
        if let Some(path) = bed.path.to_str() {
            bed.path = std::path::PathBuf::from(replace_tokens(path.to_string()));
        }
        if let Some(local_path) = &bed.local_path {
            let s = replace_tokens(local_path.to_string_lossy().to_string());
            bed.local_path = Some(std::path::PathBuf::from(s));
        }
    }
}

fn legacy_bed_annotation_key(bed: &crate::parse::bed::BedConfig) -> String {
    let candidate = bed
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| "bed".to_string());

    let sanitized = candidate
        .trim_end_matches(".gz")
        .trim_end_matches(".bgz")
        .trim_end_matches(".bedGraph")
        .trim_end_matches(".bed")
        .to_string();

    if sanitized.is_empty() {
        "bed".to_string()
    } else {
        format!("bed_{}", sanitized)
    }
}

pub fn staged_import_config_to_legacy_config(staged: &StagedImportConfig) -> ImportConfig {
    let assembly = staged.assembly.clone();
    let busco_tallies = staged
        .import
        .as_ref()
        .and_then(|import_opts| import_opts.busco_tallies.clone());
    let busco_lineages = busco_tallies
        .as_ref()
        .map(|cfg| cfg.lineages.clone())
        .unwrap_or_default();

    let busco_source = staged
        .annotations
        .get("busco")
        .map(|annotation| annotation.source.clone());

    let busco_files = busco_source
        .map(|source| {
            let paths = if busco_lineages.is_empty() {
                vec![String::new()]
            } else {
                busco_lineages
            };

            let mut files = Vec::new();
            for lineage in paths {
                let path = source
                    .path
                    .clone()
                    .map(|path| {
                        let path = path.to_string_lossy().to_string();
                        path.replace("{ACCESSION}", &assembly.accession)
                            .replace("{TAXON}", &assembly.taxon_id.clone().unwrap_or_default())
                            .replace("{TAXON_ID}", &assembly.taxon_id.clone().unwrap_or_default())
                            .replace("{LINEAGE}", &lineage)
                    })
                    .unwrap_or_default();
                let local_path = source.local_path.clone().map(|path| {
                    let path = path.to_string_lossy().to_string();
                    path.replace("{ACCESSION}", &assembly.accession)
                        .replace("{TAXON}", &assembly.taxon_id.clone().unwrap_or_default())
                        .replace("{TAXON_ID}", &assembly.taxon_id.clone().unwrap_or_default())
                        .replace("{LINEAGE}", &lineage)
                });

                if !path.trim().is_empty() {
                    files.push(crate::parse::busco::BuscoFileConfig {
                        path: std::path::PathBuf::from(path),
                        local_path: local_path.map(std::path::PathBuf::from),
                        lineage: lineage.clone(),
                        taxon_id: assembly.taxon_id.clone().unwrap_or_default(),
                        accession: assembly.accession.clone(),
                        ancestors: assembly.ancestors.clone(),
                    });
                }
            }
            files
        })
        .unwrap_or_default();

    let busco_algs = staged
        .annotations
        .get("busco")
        .and_then(|annotation| annotation.algs.clone())
        .or_else(|| {
            staged
                .annotations
                .values()
                .find_map(|annotation| annotation.algs.clone())
        });

    ImportConfig {
        assembly: assembly.clone(),
        es: staged.es.clone(),
        sequence_report: SequenceReportImportConfig {
            accession: assembly.accession.clone(),
            taxon_id: assembly.taxon_id.clone().unwrap_or_default(),
            ancestors: assembly.ancestors.clone(),
            path: staged.sequence.report.path.clone(),
            local_path: staged.sequence.report.local_path.clone(),
        },
        bed: crate::parse::bed::MultiBedConfig {
            accession: assembly.accession.clone(),
            taxon_id: assembly.taxon_id.clone().unwrap_or_default(),
            ancestors: assembly.ancestors.clone(),
            lines_per_unit: staged.windowing.lines_per_unit,
            bed_configs: staged.windowing.files.clone(),
            window_specs: staged.windowing.windows.clone(),
        },
        busco: crate::parse::busco::MultiBuscoConfig {
            accession: assembly.accession.clone(),
            taxon_id: assembly.taxon_id.clone().unwrap_or_default(),
            ancestors: assembly.ancestors.clone(),
            tables: None,
            files: if busco_files.is_empty() {
                None
            } else {
                Some(busco_files)
            },
            algs: busco_algs,
        },
        import: staged.import.clone(),
    }
}

#[deprecated(note = "compatibility bridge for legacy config; remove after staged config migration")]
pub fn normalize_legacy_import_config(cfg: &ImportConfig) -> StagedImportConfig {
    let sequence_report = ResolvedPathConfig {
        path: cfg.sequence_report.path.clone(),
        local_path: cfg.sequence_report.local_path.clone(),
    };

    let windowing = WindowingConfig {
        lines_per_unit: cfg.bed.lines_per_unit,
        windows: cfg.bed.window_specs.clone(),
        files: cfg.bed.bed_configs.clone(),
        target_size: None,
        bed_resolution: Some(cfg.bed.lines_per_unit),
        remnant_policy: None,
        remnant_bounds: None,
    };

    let mut metadata = HashMap::new();
    metadata.insert(
        "sequence_report".to_string(),
        AnnotationSourceConfig {
            source: sequence_report.clone(),
            assign_to: vec!["sequence".to_string()],
            fields: HashMap::new(),
            algs: None,
        },
    );

    let mut annotations = HashMap::new();
    for bed in &cfg.bed.bed_configs {
        annotations.insert(
            legacy_bed_annotation_key(bed),
            AnnotationSourceConfig {
                source: annotation_source_from_path(Some(bed.path.clone()), bed.local_path.clone()),
                assign_to: vec!["window".to_string()],
                fields: HashMap::new(),
                algs: None,
            },
        );
    }

    if let Some(tables) = &cfg.busco.tables {
        if let Some(table) = tables.first() {
            annotations.insert(
                "busco".to_string(),
                AnnotationSourceConfig {
                    source: annotation_source_from_path(
                        Some(table.path.clone()),
                        table.local_path.clone(),
                    ),
                    assign_to: vec!["feature".to_string()],
                    fields: HashMap::new(),
                    algs: None,
                },
            );
        }
    }
    if !annotations.contains_key("busco") {
        if let Some(files) = &cfg.busco.files {
            if let Some(file) = files.first() {
                annotations.insert(
                    "busco".to_string(),
                    AnnotationSourceConfig {
                        source: annotation_source_from_path(
                            Some(file.path.clone()),
                            file.local_path.clone(),
                        ),
                        assign_to: vec!["feature".to_string()],
                        fields: HashMap::new(),
                        algs: None,
                    },
                );
            }
        }
    }

    let mut staged = StagedImportConfig {
        assembly: cfg.assembly.clone(),
        es: cfg.es.clone(),
        sequence: SequenceMetadataConfig {
            report: sequence_report,
            metadata,
        },
        annotations,
        windowing,
        assembly_policy: Some(AssemblyPolicyConfig {
            min_chromosome_fraction: 0.9,
            fallback_mode: "minimal".to_string(),
        }),
        scaffold_policy: Some(ScaffoldPolicyConfig {
            min_scaffold_length: 1_000_000,
            skip_short_scaffolds_without_data: true,
            index_small_scaffold_as_parent_if: vec!["busco".to_string()],
        }),
        indexing: Some(IndexingConfig {
            profile: "standard".to_string(),
        }),
        derived_metrics: vec![DerivedMetricConfig {
            name: "distance_to_telomere".to_string(),
            target: "window".to_string(),
            source: "sequence".to_string(),
            anchor: Some("midpoint".to_string()),
            output_type: Some("float".to_string()),
            flags: vec![],
        }],
        import: cfg.import.clone(),
    };
    normalize_staged_import_config(&mut staged);
    staged
}

pub fn validate_staged_import_config(staged: &StagedImportConfig) -> Result<(), anyhow::Error> {
    crate::config::validation::validate_staged_import_config(staged)
}

pub fn validate_legacy_runtime_matches_staged(
    cfg: &ImportConfig,
    staged: &StagedImportConfig,
) -> Result<(), anyhow::Error> {
    if !staged.sequence.metadata.contains_key("sequence_report") {
        return Ok(());
    }

    let bed_keys = cfg
        .bed
        .bed_configs
        .iter()
        .map(legacy_bed_annotation_key)
        .collect::<Vec<_>>();

    if cfg.bed.bed_configs.len() != staged.windowing.files.len() {
        return Err(anyhow::anyhow!(
            "legacy BED sources ({}) do not match staged windowing files ({})",
            cfg.bed.bed_configs.len(),
            staged.windowing.files.len()
        ));
    }

    for key in &bed_keys {
        if !staged.annotations.contains_key(key) {
            return Err(anyhow::anyhow!(
                "staged config is missing the legacy BED annotation entry for {}",
                key
            ));
        }
    }

    let has_busco = cfg.busco.tables.is_some() || cfg.busco.files.is_some();
    if has_busco && !staged.annotations.contains_key("busco") {
        return Err(anyhow::anyhow!(
            "staged config is missing the legacy BUSCO annotation entry"
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_normalizes_to_staged_model() {
        let cfg = ImportConfig {
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
            sequence_report: crate::import::SequenceReportImportConfig {
                accession: "GCA_00000001.1".to_string(),
                taxon_id: "12345".to_string(),
                ancestors: vec![],
                path: None,
                local_path: Some(std::path::PathBuf::from("/tmp/sequence_report.jsonl")),
            },
            bed: crate::parse::bed::MultiBedConfig {
                accession: "GCA_00000001.1".to_string(),
                taxon_id: "12345".to_string(),
                ancestors: vec![],
                lines_per_unit: 1000,
                bed_configs: vec![crate::parse::bed::BedConfig {
                    path: std::path::PathBuf::from("/tmp/gc.bed.gz"),
                    local_path: None,
                    value_columns: vec![],
                    has_header: false,
                }],
                window_specs: vec![crate::parse::bed::WindowSpec::Size {
                    size: 1_000_000,
                    remnant_policy: crate::parse::bed::RemnantPolicy::Centered,
                    remnant_bounds: None,
                }],
            },
            busco: crate::parse::busco::MultiBuscoConfig {
                accession: "GCA_00000001.1".to_string(),
                taxon_id: "12345".to_string(),
                ancestors: vec![],
                tables: Some(vec![crate::parse::busco::BuscoTableConfig {
                    path: std::path::PathBuf::from("/tmp/busco/full_table.tsv"),
                    local_path: None,
                    lineages: Some(vec!["diptera_odb12".to_string()]),
                }]),
                files: None,
                algs: None,
            },
            import: None,
        };

        let staged = normalize_legacy_import_config(&cfg);
        assert_eq!(staged.windowing.lines_per_unit, 1000);
        assert!(!staged.windowing.files.is_empty());
        assert!(staged.sequence.report.local_path.is_some());
        assert!(staged.sequence.metadata.contains_key("sequence_report"));
        assert!(staged.annotations.contains_key("bed_gc"));
        assert!(staged.annotations.contains_key("busco"));
        assert_eq!(staged.derived_metrics[0].name, "distance_to_telomere");
        assert_eq!(
            staged.windowing.files[0].path,
            std::path::PathBuf::from("/tmp/gc.bed.gz")
        );
    }

    #[test]
    fn staged_config_deserializes_and_converts_to_legacy_runtime() {
        let yaml = r#"
assembly:
  accession: GCA_016920705.1
  taxon_id: 7157
es:
  host: "http://localhost"
  port: 9200
  hub:
    name: goat
    release: 2021.10.15
    taxonomy: ncbi
sequence:
  report:
    path: "https://example.org/{ACCESSION}/sequence_report.jsonl"
    local_path: "~/tmp/{ACCESSION}.sequence_report.jsonl"
annotations:
  busco:
    source:
      path: "https://gap.cog.sanger.ac.uk/{ACCESSION}/busco/{LINEAGE}/{ACCESSION}.{LINEAGE}.full_table.tsv.gz"
      local_path: "~/tmp/{ACCESSION}.{LINEAGE}.full_table.tsv.gz"
    assign_to: [feature]
    fields: {}
    algs:
      - name: diptera_alg
        lineage: diptera_odb12
        path: "https://raw.githubusercontent.com/Obscuromics/diptera-ALGs/refs/heads/main/tables/ALGs_syngraph_diptera.tsv"
        local_path: "~/tmp/diptera_alg.tsv"
windowing:
  lines_per_unit: 1000
  windows:
    - type: size
      size: 1000000
      remnant_policy: Centered
    - type: proportion
      proportion: 0.1
  files:
    - path: "https://gap.cog.sanger.ac.uk/{ACCESSION}/base_content/k1/{ACCESSION}.GC.1k.bedGraph.gz"
      local_path: "~/tmp/{ACCESSION}.GC.1k.bedGraph.gz"
      value_columns:
        - label: gc
          index: 3
          type: float
          summary_functions:
            - name: mean
import:
  entity_types:
    - sequence
    - window
    - busco
    - attribute
  busco_tallies:
    lineages:
      - diptera_odb12
    assembly_counts_output: "./busco_assembly_counts.tsv"
  synteny_index:
    enrich_busco_features: true
    index_synteny_loci: true
    index_synteny_blocks: true
"#;

        let mut staged: StagedImportConfig = serde_yaml::from_str(yaml).unwrap();
        expand_staged_placeholders(&mut staged);
        validate_staged_import_config(&staged).unwrap();

        let cfg = crate::config::legacy::staged_import_config_to_legacy_config(&staged);
        assert_eq!(
            cfg.sequence_report.local_path,
            Some(std::path::PathBuf::from(
                "~/tmp/GCA_016920705.1.sequence_report.jsonl"
            ))
        );
        assert_eq!(cfg.bed.window_specs.len(), 2);
        assert!(cfg.busco.algs.is_some());
        assert_eq!(cfg.busco.algs.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn staged_sequence_report_template_expands_before_validation() {
        let yaml = r#"
assembly:
  accession: GCA_016920715.1
  taxon_id: 7173
es:
  host: "http://localhost"
  port: 9200
  hub:
    name: goat
    release: 2021.10.15
    taxonomy: ncbi
sequence:
  report:
    local_path: "~/tmp/{ACCESSION}.sequence_report.jsonl"
windowing:
  lines_per_unit: 1000
  windows:
    - type: size
      size: 1000000
      remnant_policy: Centered
  files:
    - path: "https://gap.cog.sanger.ac.uk/{ACCESSION}/base_content/k1/{ACCESSION}.GC.1k.bedGraph.gz"
      local_path: "~/tmp/{ACCESSION}.GC.1k.bedGraph.gz"
      value_columns:
        - label: gc
          index: 3
          type: float
          summary_functions:
            - name: mean
"#;

        let mut staged: StagedImportConfig = serde_yaml::from_str(yaml).unwrap();
        expand_staged_placeholders(&mut staged);
        validate_staged_import_config(&staged).unwrap();

        let cfg = staged_import_config_to_legacy_config(&staged);
        assert_eq!(
            cfg.sequence_report.local_path,
            Some(std::path::PathBuf::from(
                "~/tmp/GCA_016920715.1.sequence_report.jsonl"
            ))
        );
    }
}
