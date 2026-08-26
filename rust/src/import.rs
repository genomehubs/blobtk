//!
//! Invoked by calling:
//! `blobtk import <args>`

// use crate::index::es::config;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error;
use crate::index::es::client::ElasticsearchClient;
use crate::index::es::models::attribute_builder::build_attribute_document;
use crate::index::es::models::documents::{AttributeDocument, FeatureDocument};
use crate::index::es::models::nested_documents::NestedAttribute;
use crate::parse::bed::{parse_bed_files, MultiBedConfig};
use crate::parse::busco::{
    attributes::SyntenyIndexMode, parse_busco_files, BlockSetMetrics, BuscoFileConfig,
    MultiBuscoConfig,
};
use crate::parse::sequence_report;
use std::collections::HashMap;

pub mod state;
use state::ImportState;

#[derive(Deserialize, Serialize, Debug)]
pub struct HubConfig {
    pub name: String,
    pub release: String,
    pub taxonomy: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct EsConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub hub: HubConfig,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct SequenceReportImportConfig {
    pub accession: String,
    pub taxon_id: String,
    pub path: Option<std::path::PathBuf>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct AssemblyImportConfig {
    pub accession: String,
    pub taxon_id: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ImportOptions {
    pub entity_types: Option<Vec<String>>, // ["sequence", "window", "busco"]
    pub busco_tallies: Option<BuscoTalliesConfig>,
    pub synteny_index: Option<SyntenyIndexMode>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BuscoTalliesConfig {
    pub lineages: Vec<String>,
    pub assembly_counts_output: Option<std::path::PathBuf>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ImportConfig {
    pub assembly: AssemblyImportConfig,
    pub es: EsConfig,
    pub sequence_report: SequenceReportImportConfig,
    pub bed: MultiBedConfig,
    pub busco: MultiBuscoConfig,
    pub import: Option<ImportOptions>,
}

fn expand_busco_tables(cfg: &mut ImportConfig) {
    let accession = cfg.assembly.accession.clone();
    let taxon = cfg.assembly.taxon_id.clone();
    let mut expanded: Vec<BuscoFileConfig> = Vec::new();

    // iterate over original table entries (adapt field name to your struct)
    if let Some(tables) = &cfg.busco.tables {
        for table in tables.iter() {
            // normalize path string
            let path_str = table.path.to_string_lossy().to_string();

            if let Some(lineages) = &table.lineages {
                for lineage in lineages {
                    let p = path_str
                        .replace("{ACCESSION}", &accession)
                        .replace("{LINEAGE}", lineage)
                        .replace("{TAXON}", &taxon);
                    expanded.push(BuscoFileConfig {
                        path: PathBuf::from(p),
                        lineage: lineage.clone(),
                        taxon_id: taxon.clone(),
                        accession: accession.clone(),
                    });
                }
            } else {
                let p = path_str
                    .replace("{ACCESSION}", &accession)
                    .replace("{TAXON}", &taxon);
                expanded.push(BuscoFileConfig {
                    path: PathBuf::from(p),
                    lineage: String::new(),
                    taxon_id: taxon.clone(),
                    accession: accession.clone(),
                });
            }
        }
    }
    cfg.busco.files = Some(expanded);
}

fn expand_placeholders(cfg: &mut ImportConfig) {
    let accession = cfg.assembly.accession.clone();
    for bed in cfg.bed.bed_configs.iter_mut() {
        let s = bed.path.to_string_lossy().to_string();
        let s = s.replace("{ACCESSION}", &accession);
        bed.path = std::path::PathBuf::from(s);
    }
    expand_busco_tables(cfg);
    let s = cfg
        .sequence_report
        .path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());
    if let Some(s) = s {
        let s = s.replace("{ACCESSION}", &accession);
        cfg.sequence_report.path = Some(std::path::PathBuf::from(s));
    }
}

fn attach_busco_category_counts(state: &mut ImportState) -> Result<(), anyhow::Error> {
    // For each unique BUSCO ID (not occurrence)
    for busco_id in state.busco_id_tracker.occurrences.keys() {
        if let Some(occurrences) = state.busco_id_tracker.occurrences.get(busco_id) {
            // Get lineage from first occurrence
            let lineage = &occurrences[0].3;
            let categories = state.busco_id_tracker.categorize(busco_id, lineage);

            // Add to assembly counts ONCE per BUSCO ID
            for category in &categories {
                state.busco_counts.add_to_assembly(lineage, category);
            }

            // Add to sequence/window counts for each occurrence
            for (seq_id, window_ids, _, _) in occurrences {
                for category in &categories {
                    state
                        .busco_counts
                        .add_to_sequence(seq_id, lineage, category);
                    for win_id in window_ids {
                        state.busco_counts.add_to_window(win_id, lineage, category);
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn sync_attribute_documents(
    docs: Vec<AttributeDocument>,
    state: &mut ImportState,
    es_cfg: &EsConfig,
    import_opts: &Option<ImportOptions>,
) -> Result<(), error::Error> {
    let should_index = import_opts
        .as_ref()
        .and_then(|io| io.entity_types.as_ref())
        .map_or(true, |et| et.contains(&"attribute".to_string()));

    let mut merged_docs = Vec::new();
    for doc in docs {
        if let Some(doc) = state.attribute_doc_cache.merge_or_insert(doc)? {
            merged_docs.push(doc);
        }
    }

    if should_index && !merged_docs.is_empty() {
        let client = ElasticsearchClient::try_from(es_cfg)?;
        eprintln!("    Indexing {} attribute documents", merged_docs.len());
        let wrapped_docs = client.wrap_for_bulk_index(merged_docs)?;
        client.index_documents("attributes", wrapped_docs)?;
        client.refresh("attributes")?;
    }

    Ok(())
}

fn restore_attribute_cache(state: &mut ImportState, es_cfg: &EsConfig) -> Result<(), error::Error> {
    let client = ElasticsearchClient::try_from(es_cfg)?;
    let index_name = client.resolve_index_name("attributes")?;
    let response = match client.search(
        &index_name,
        serde_json::json!({
            "size": 10000,
            "query": { "match_all": {} }
        }),
    ) {
        Ok(response) => response,
        Err(err) if err.to_string().contains("index_not_found_exception") => return Ok(()),
        Err(err) => return Err(err.into()),
    };

    if let Some(hits) = response
        .get("hits")
        .and_then(|hits| hits.get("hits"))
        .and_then(|hits| hits.as_array())
    {
        for hit in hits {
            if let Some(source) = hit.get("_source") {
                if let Some(document) = attribute_document_from_cache_source(source) {
                    state.attribute_doc_cache.register_existing(document);
                }
            }
        }
    }

    Ok(())
}

fn attribute_document_from_cache_source(source: &serde_json::Value) -> Option<AttributeDocument> {
    let name = source.get("name")?.as_str()?.to_string();
    let group = match source.get("group").and_then(|value| value.as_str()) {
        Some("feature") => crate::index::es::models::IndexGroup::Feature,
        Some("taxon") => crate::index::es::models::IndexGroup::Taxon,
        Some("assembly") => crate::index::es::models::IndexGroup::Assembly,
        Some("sample") => crate::index::es::models::IndexGroup::Sample,
        Some("attribute") => crate::index::es::models::IndexGroup::Attribute,
        _ => crate::index::es::models::IndexGroup::Feature,
    };
    let field_type = match source.get("type").and_then(|value| value.as_str()) {
        Some("boolean") => crate::validation::spec::FieldType::Boolean,
        Some("byte") => crate::validation::spec::FieldType::Byte,
        Some("date") => crate::validation::spec::FieldType::Date,
        Some("double") => crate::validation::spec::FieldType::Double,
        Some("float") => crate::validation::spec::FieldType::Float,
        Some("geo_point") => crate::validation::spec::FieldType::GeoPoint,
        Some("half_float") => crate::validation::spec::FieldType::HalfFloat,
        Some("integer") => crate::validation::spec::FieldType::Integer,
        Some("long") => crate::validation::spec::FieldType::Long,
        Some("short") => crate::validation::spec::FieldType::Short,
        Some("1dp") => crate::validation::spec::FieldType::OneDP,
        Some("2dp") => crate::validation::spec::FieldType::TwoDP,
        Some("3dp") => crate::validation::spec::FieldType::ThreeDP,
        Some("4dp") => crate::validation::spec::FieldType::FourDP,
        _ => crate::validation::spec::FieldType::Keyword,
    };

    Some(AttributeDocument {
        group,
        name,
        field_type,
        display_name: source
            .get("display_name")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        description: source
            .get("description")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        constraint: source.get("constraint").cloned(),
        ..Default::default()
    })
}

fn attach_synteny_metrics_to_attributes(
    attrs: &mut Vec<NestedAttribute>,
    sequence_id: &str,
    synteny_metrics_by_seq: &HashMap<String, BlockSetMetrics>,
) {
    if let Some(metrics) = synteny_metrics_by_seq.get(sequence_id) {
        let block_set_attrs = metrics.to_nested_attribute_docs();
        attrs.extend(block_set_attrs);
    }
}

fn attach_active_window_synteny_metrics_to_attributes(
    attrs: &mut Vec<NestedAttribute>,
    window_id: &str,
    synteny_metrics_by_window: &HashMap<String, BlockSetMetrics>,
) {
    if let Some(metrics) = synteny_metrics_by_window.get(window_id) {
        let block_set_attrs = metrics.to_active_attribute_docs();
        attrs.extend(block_set_attrs);
    }
}

fn attach_counts_and_index_sequences(
    state: &mut ImportState,
    es_cfg: &EsConfig,
    import_opts: &Option<ImportOptions>,
) -> Result<(), error::Error> {
    let client = ElasticsearchClient::try_from(es_cfg)?;

    // Attach tallied counts to each sequence
    for (seq_id, seq_doc) in state.sequences.iter_mut() {
        if let Some(lineage_counts) = state.busco_counts.seq_counts.get(seq_id) {
            let mut attrs = seq_doc.attributes.take().unwrap_or_default();
            for (lineage, status_counts) in lineage_counts {
                for (status, count) in status_counts {
                    let attr_key = format!("{}_{}_count", lineage, status);
                    attrs.push(NestedAttribute {
                        key: attr_key,
                        integer_value: Some(*count as i32),
                        ..Default::default()
                    });
                }
            }
            attach_synteny_metrics_to_attributes(&mut attrs, seq_id, &state.synteny_metrics_by_seq);

            seq_doc.attributes = Some(attrs);
        }
    }

    // Index sequences
    let seq_docs: Vec<_> = state.sequences.values().cloned().collect();
    if !seq_docs.is_empty() {
        eprintln!("  Indexing {} sequence features", seq_docs.len());

        // Create AttributeDocuments for newly added counts
        create_attribute_docs_from_features(&seq_docs, state, es_cfg, import_opts)?;

        let wrapped_docs = client.wrap_for_bulk_index(seq_docs)?;
        client.index_documents("feature", wrapped_docs)?;
        client.refresh("feature")?;
    }

    Ok(())
}

fn parse_bed_and_index(
    bed_cfg: &MultiBedConfig,
    state: &mut ImportState,
    es_cfg: &EsConfig,
    import_opts: &Option<ImportOptions>,
) -> Result<(), error::Error> {
    let client = ElasticsearchClient::try_from(es_cfg)?;

    let window_docs = parse_bed_files(bed_cfg)?;

    // Attach tallied BUSCO counts to windows.
    // Intentionally do not attach sequence-level synteny summary metrics here:
    // window features must keep their own local stats and must not inherit the
    // parent sequence's compact summary values.
    let mut window_docs_final = window_docs;
    for (window_id, window_doc) in window_docs_final.iter_mut() {
        let mut attrs = window_doc.attributes.take().unwrap_or_default();

        if let Some(lineage_counts) = state.busco_counts.window_counts.get(window_id) {
            for (lineage, status_counts) in lineage_counts {
                for (status, count) in status_counts {
                    let attr_key = format!("{}_{}_count", lineage, status);
                    attrs.push(NestedAttribute {
                        key: attr_key,
                        integer_value: Some(*count as i32),
                        ..Default::default()
                    });
                }
            }
        }

        attach_active_window_synteny_metrics_to_attributes(
            &mut attrs,
            window_id,
            &state.synteny_metrics_by_window,
        );

        window_doc.attributes = Some(attrs);
    }

    // Index windows
    let window_vec: Vec<_> = window_docs_final.values().cloned().collect();
    if !window_vec.is_empty() {
        eprintln!("  Indexing {} window features", window_vec.len());

        // Create AttributeDocuments for newly added counts
        create_attribute_docs_from_features(&window_vec, state, es_cfg, import_opts)?;

        let wrapped_docs = client.wrap_for_bulk_index(window_vec)?;
        client.index_documents("feature", wrapped_docs)?;
        client.refresh("feature")?;
    }

    Ok(())
}

fn write_assembly_busco_counts(
    state: &ImportState,
    output_path: &std::path::PathBuf,
) -> Result<(), anyhow::Error> {
    use std::io::Write;

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = std::fs::File::create(output_path)?;

    // Use assembly_counts directly (not seq aggregation)
    let mut lineages: Vec<_> = state.busco_counts.assembly_counts.keys().cloned().collect();
    lineages.sort();

    let mut all_statuses = std::collections::HashSet::new();
    for lineage_categories in state.busco_counts.assembly_counts.values() {
        for status in lineage_categories.keys() {
            all_statuses.insert(status.clone());
        }
    }
    let mut all_statuses: Vec<_> = all_statuses.into_iter().collect();
    all_statuses.sort();

    // Write header
    write!(file, "taxon_id\tassembly_id")?;
    for lineage in &lineages {
        for status in &all_statuses {
            write!(file, "\t{}_{}", lineage, status)?;
        }
    }
    writeln!(file)?;

    // Write data row
    write!(file, "{}\t{}", state.taxon_id, state.assembly_id)?;
    for lineage in &lineages {
        for status in &all_statuses {
            let count = state
                .busco_counts
                .assembly_counts
                .get(lineage)
                .and_then(|lc| lc.get(status))
                .copied()
                .unwrap_or(0);
            write!(file, "\t{}", count)?;
        }
    }
    writeln!(file)?;

    eprintln!("  Written assembly counts to {}", output_path.display());
    Ok(())
}

fn create_attribute_docs_from_features(
    features: &[FeatureDocument],
    state: &mut ImportState,
    es_cfg: &EsConfig,
    import_opts: &Option<ImportOptions>,
) -> Result<(), error::Error> {
    let mut attribute_docs = Vec::new();

    for feature in features {
        if let Some(attrs) = &feature.attributes {
            for attr in attrs {
                let overrides =
                    crate::index::es::models::attribute_builder::feature_attribute_overrides(attr);
                attribute_docs.push(build_attribute_document(attr, Some(&overrides)));
            }
        }
    }

    sync_attribute_documents(attribute_docs, state, es_cfg, import_opts)
}

pub fn import(options: &crate::cli::ImportOptions) -> Result<(), anyhow::Error> {
    let config_path = &options.config;
    let yaml_text = std::fs::read_to_string(config_path)?;
    let mut cfg: ImportConfig = serde_yaml::from_str(&yaml_text)?;
    expand_placeholders(&mut cfg);

    let assembly_id = cfg.assembly.accession.clone();
    let taxon_id = cfg.assembly.taxon_id.clone();
    let mut import_state = ImportState::new(assembly_id, taxon_id);
    restore_attribute_cache(&mut import_state, &cfg.es)?;

    eprintln!("Step 1: Parsing sequence report...");
    let sequence_report_cfg = cfg.sequence_report;
    let sequence_features = sequence_report::parse_sequence_report(sequence_report_cfg)?;
    import_state.sequences = sequence_features.clone();

    // Create AttributeDocuments for sequence features
    let seq_vec: Vec<_> = import_state.sequences.values().cloned().collect();
    create_attribute_docs_from_features(&seq_vec, &mut import_state, &cfg.es, &cfg.import)?;

    let window_cfg = cfg.bed.window_specs.clone();
    let busco_cfg = cfg.busco;
    parse_busco_files(
        &busco_cfg,
        &sequence_features,
        window_cfg,
        cfg.bed.lines_per_unit,
        &mut import_state,
        &cfg.es,
        &cfg.import,
        cfg.import
            .as_ref()
            .and_then(|import_opts| import_opts.synteny_index.as_ref()),
    )?;
    attach_busco_category_counts(&mut import_state)?;

    eprintln!("Step 3: Attaching counts to sequences and indexing...");
    attach_counts_and_index_sequences(&mut import_state, &cfg.es, &cfg.import)?;

    eprintln!("Step 4: Parsing BED files and creating windows...");
    parse_bed_and_index(&cfg.bed, &mut import_state, &cfg.es, &cfg.import)?;
    // ========== STEP 5: Write assembly-level busco counts ==========
    if let Some(import_opts) = &cfg.import {
        if let Some(tally_cfg) = &import_opts.busco_tallies {
            if let Some(output_path) = &tally_cfg.assembly_counts_output {
                eprintln!("Step 5: Writing assembly-level BUSCO counts...");
                write_assembly_busco_counts(&import_state, output_path)?;
            }
        }
    }

    eprintln!("Import complete!");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::es::models::nested_documents::NestedAttribute;

    #[test]
    fn attach_synteny_metrics_to_window_attrs_includes_active_compact_summary_fields() {
        let mut attrs = vec![NestedAttribute {
            key: "busco_status_count".to_string(),
            integer_value: Some(3),
            ..Default::default()
        }];
        let mut synteny_metrics_by_seq = HashMap::new();
        synteny_metrics_by_seq.insert(
            "seq-1".to_string(),
            BlockSetMetrics {
                total_loci: 8,
                distinct_group_count: 2,
                longest_block_size: 5,
                block_count: 2,
                majority_group_count: 5,
                majority_group_id: Some("group-A".to_string()),
                majority_group_fraction: Some(0.625),
                majority_group_threshold_flag: true,
                filtered_transition_count_ratio: Some(0.5),
                filtered_gini_score: Some(0.4),
                normalised_transition_count_ratio: 0.0,
                normalised_gini_score: 0.0,
                normalised_minority_gini_score: 0.0,
                normalised_block_size: 0.0,
                normalised_distinct_group_count: 0.0,
                normalised_block_count: 0.0,
                normalised_interminority_transition_ratio: 0.0,
            },
        );

        attach_active_window_synteny_metrics_to_attributes(
            &mut attrs,
            "seq-1",
            &synteny_metrics_by_seq,
        );

        let keys: Vec<_> = attrs.iter().map(|attr| attr.key.clone()).collect();
        assert!(keys.contains(&"total_loci".to_string()));
        assert!(keys.contains(&"majority_group_id".to_string()));
        assert!(keys.contains(&"majority_group_fraction".to_string()));
        assert!(keys.contains(&"filtered_transition_count_ratio".to_string()));
        assert!(keys.contains(&"filtered_gini_score".to_string()));
        assert!(!keys.contains(&"normalised_transition_count_ratio".to_string()));
        assert!(!keys.contains(&"normalised_gini_score".to_string()));
        assert!(attrs.iter().all(|attr| attr.deprecated != Some(true)));
    }

    #[test]
    fn filtered_gini_score_is_blank_for_single_locus_data() {
        let block_set = crate::parse::busco::SyntenyBlockSet {
            group_set_id: "lineage".to_string(),
            sequence_id: "seq-1".to_string(),
            assembly_id: "asm".to_string(),
            taxon_id: "tax".to_string(),
            blocks: vec![],
            counts: std::collections::HashMap::from([("group-A".to_string(), 1)]),
            total_loci: 1,
            distinct_group_count: 1,
            longest_block_size: 1,
            latest_group_id: Some("group-A".to_string()),
            loci: vec![],
            metrics: None,
            transitions: None,
        };

        assert!(block_set.filtered_gini_score().is_none());

        let metrics = BlockSetMetrics {
            total_loci: 1,
            distinct_group_count: 1,
            longest_block_size: 1,
            block_count: 1,
            majority_group_count: 1,
            majority_group_id: Some("group-A".to_string()),
            majority_group_fraction: Some(1.0),
            majority_group_threshold_flag: true,
            filtered_transition_count_ratio: None,
            filtered_gini_score: None,
            normalised_transition_count_ratio: 0.0,
            normalised_gini_score: 0.0,
            normalised_minority_gini_score: 0.0,
            normalised_block_size: 0.0,
            normalised_distinct_group_count: 0.0,
            normalised_block_count: 0.0,
            normalised_interminority_transition_ratio: 0.0,
        };

        let docs = metrics.to_nested_attribute_docs();
        assert!(!docs.iter().any(|doc| doc.key == "filtered_gini_score"));
        assert!(!docs.iter().any(|doc| doc.key == "normalised_gini_score"));
    }

    #[test]
    fn imported_window_metrics_use_window_specific_values() {
        let mut state = ImportState::new("asm-1".to_string(), "tax-1".to_string());
        state.synteny_metrics_by_window.insert(
            "win-1".to_string(),
            BlockSetMetrics {
                total_loci: 3,
                distinct_group_count: 2,
                longest_block_size: 2,
                block_count: 2,
                majority_group_count: 2,
                majority_group_id: Some("group-B".to_string()),
                majority_group_fraction: Some(0.67),
                majority_group_threshold_flag: true,
                filtered_transition_count_ratio: Some(0.5),
                filtered_gini_score: Some(0.33),
                normalised_transition_count_ratio: 0.0,
                normalised_gini_score: 0.0,
                normalised_minority_gini_score: 0.0,
                normalised_block_size: 0.0,
                normalised_distinct_group_count: 0.0,
                normalised_block_count: 0.0,
                normalised_interminority_transition_ratio: 0.0,
            },
        );
        state.synteny_metrics_by_seq.insert(
            "seq-1".to_string(),
            BlockSetMetrics {
                total_loci: 20,
                distinct_group_count: 2,
                longest_block_size: 10,
                block_count: 2,
                majority_group_count: 12,
                majority_group_id: Some("group-A".to_string()),
                majority_group_fraction: Some(0.6),
                majority_group_threshold_flag: true,
                filtered_transition_count_ratio: Some(0.4),
                filtered_gini_score: Some(0.25),
                normalised_transition_count_ratio: 0.0,
                normalised_gini_score: 0.0,
                normalised_minority_gini_score: 0.0,
                normalised_block_size: 0.0,
                normalised_distinct_group_count: 0.0,
                normalised_block_count: 0.0,
                normalised_interminority_transition_ratio: 0.0,
            },
        );

        let mut attrs = vec![];
        attach_active_window_synteny_metrics_to_attributes(
            &mut attrs,
            "win-1",
            &state.synteny_metrics_by_window,
        );

        let majority_group_id = attrs
            .iter()
            .find(|attr| attr.key == "majority_group_id")
            .and_then(|attr| attr.keyword_value.as_ref())
            .and_then(|value| match value {
                crate::parse::genomehubs::StringOrVec::Single(v) => Some(v.clone()),
                crate::parse::genomehubs::StringOrVec::Multiple(v) => v.first().cloned(),
            });

        assert_eq!(majority_group_id.as_deref(), Some("group-B"));
        assert!(attrs
            .iter()
            .any(|attr| attr.key == "total_loci" && attr.integer_value == Some(3)));
        assert!(attrs.iter().all(|attr| attr.key != "normalised_gini_score"));
    }

    #[test]
    fn window_attributes_receive_basic_compact_summary_fields() {
        let mut attrs = vec![NestedAttribute {
            key: "lineage_status_count".to_string(),
            integer_value: Some(7),
            ..Default::default()
        }];
        let metrics_by_seq = std::collections::HashMap::from([(
            "seq-1".to_string(),
            BlockSetMetrics {
                total_loci: 20,
                distinct_group_count: 2,
                longest_block_size: 10,
                block_count: 2,
                majority_group_count: 12,
                majority_group_id: Some("group-A".to_string()),
                majority_group_fraction: Some(0.6),
                majority_group_threshold_flag: true,
                filtered_transition_count_ratio: Some(0.4),
                filtered_gini_score: Some(0.25),
                normalised_transition_count_ratio: 0.0,
                normalised_gini_score: 0.0,
                normalised_minority_gini_score: 0.0,
                normalised_block_size: 0.0,
                normalised_distinct_group_count: 0.0,
                normalised_block_count: 0.0,
                normalised_interminority_transition_ratio: 0.0,
            },
        )]);

        attach_synteny_metrics_to_attributes(&mut attrs, "seq-1", &metrics_by_seq);

        assert!(attrs.iter().any(|attr| attr.key == "total_loci"));
        assert!(attrs.iter().any(|attr| attr.key == "majority_group_id"));
        assert!(attrs.iter().any(|attr| attr.key == "filtered_gini_score"));
        assert!(attrs.iter().all(|attr| attr.key != "normalised_gini_score"));
    }
}
