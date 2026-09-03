//!
//! Invoked by calling:
//! `blobtk import <args>`

// use crate::index::es::config;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::attribute_registry::AttributeRegistry;
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
    pub local_path: Option<std::path::PathBuf>,
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
            let local_path_str = table
                .local_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string());

            if let Some(lineages) = &table.lineages {
                for lineage in lineages {
                    let p = path_str
                        .replace("{ACCESSION}", &accession)
                        .replace("{LINEAGE}", lineage)
                        .replace("{TAXON}", &taxon);
                    let local_p = local_path_str.as_ref().map(|s| {
                        s.replace("{ACCESSION}", &accession)
                            .replace("{LINEAGE}", lineage)
                            .replace("{TAXON}", &taxon)
                    });
                    expanded.push(BuscoFileConfig {
                        path: PathBuf::from(p),
                        local_path: local_p.as_ref().map(|s| PathBuf::from(s)),
                        lineage: lineage.clone(),
                        taxon_id: taxon.clone(),
                        accession: accession.clone(),
                    });
                }
            } else {
                let p = path_str
                    .replace("{ACCESSION}", &accession)
                    .replace("{TAXON}", &taxon);
                let local_p = local_path_str.as_ref().map(|s| {
                    s.replace("{ACCESSION}", &accession)
                        .replace("{TAXON}", &taxon)
                });
                expanded.push(BuscoFileConfig {
                    path: PathBuf::from(p),
                    local_path: local_p.as_ref().map(|s| PathBuf::from(s)),
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
        if let Some(local_path) = &bed.local_path {
            let s = local_path.to_string_lossy().to_string();
            let s = s.replace("{ACCESSION}", &accession);
            bed.local_path = Some(std::path::PathBuf::from(s));
        }
    }
    expand_busco_tables(cfg);
    let s = cfg
        .sequence_report
        .local_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());
    if let Some(s) = s {
        let s = s.replace("{ACCESSION}", &accession);
        cfg.sequence_report.local_path = Some(std::path::PathBuf::from(s));
    }
}

fn ensure_index_exists(
    es_cfg: &EsConfig,
    index_prefix: &str,
    mappings: crate::index::es::mappings::common::Mappings,
) -> Result<(), error::Error> {
    let client = ElasticsearchClient::try_from(es_cfg)?;
    let index_name = client.resolve_index_name(index_prefix)?;

    match client.get_index_info(&index_name) {
        Ok(_) => return Ok(()),
        Err(err) => {
            let err_str = err.to_string();
            if !err_str.contains("not_found") && !err_str.contains("index_not_found_exception") {
                return Err(err.into());
            }
        }
    }

    let config = crate::index::es::config::IndexConfig {
        settings: Default::default(),
        mappings: Some(mappings),
    };
    match client.create_index(&index_name, config) {
        Ok(_) => {
            eprintln!(
                "  Created index {} and waiting for it to become ready",
                index_name
            );
            client.wait_for_index_ready(&index_name, "yellow")?;
            Ok(())
        }
        Err(err) => {
            let err_str = err.to_string();
            if err_str.contains("already exists")
                || err_str.contains("resource_already_exists_exception")
            {
                eprintln!("  Index {} already exists; checking readiness", index_name);
                client.wait_for_index_ready(&index_name, "yellow")?;
                Ok(())
            } else {
                Err(err.into())
            }
        }
    }
}

fn ensure_import_indices(es_cfg: &EsConfig) -> Result<(), error::Error> {
    ensure_index_exists(
        es_cfg,
        "feature",
        crate::index::es::mappings::feature_index_mappings(),
    )?;
    ensure_index_exists(
        es_cfg,
        "attributes",
        crate::index::es::mappings::attribute_index_mappings(),
    )?;
    Ok(())
}

fn attach_busco_category_counts(state: &mut ImportState) -> Result<(), anyhow::Error> {
    // Sequence-level BUSCO counts are recorded during parsing as raw status totals
    // (e.g. complete / fragmented / missing). They must not be re-aggregated here,
    // otherwise the same BUSCO loci are double-counted on the sequence feature.
    for busco_id in state.busco_id_tracker.occurrences.keys() {
        if let Some(occurrences) = state.busco_id_tracker.occurrences.get(busco_id) {
            let lineage = &occurrences[0].3;
            let categories = state.busco_id_tracker.categorize(busco_id, lineage);

            for category in &categories {
                state.busco_counts.add_to_assembly(lineage, category);
            }

            for (_, window_ids, _, _) in occurrences {
                for category in &categories {
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
        let block_set_attrs = metrics.to_active_attribute_docs();
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

fn attach_rich_group_and_transition_metrics_to_attributes(
    attrs: &mut Vec<NestedAttribute>,
    sequence_id: &str,
    synteny_metrics_by_seq: &HashMap<String, BlockSetMetrics>,
) {
    if let Some(metrics) = synteny_metrics_by_seq.get(sequence_id) {
        let rich_attrs = metrics.to_rich_attribute_docs();
        attrs.extend(rich_attrs);
    }
}

fn attach_rich_window_group_and_transition_metrics_to_attributes(
    attrs: &mut Vec<NestedAttribute>,
    window_id: &str,
    synteny_metrics_by_window: &HashMap<String, BlockSetMetrics>,
) {
    if let Some(metrics) = synteny_metrics_by_window.get(window_id) {
        let rich_attrs = metrics.to_rich_attribute_docs();
        attrs.extend(rich_attrs);
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
            attach_rich_group_and_transition_metrics_to_attributes(
                &mut attrs,
                seq_id,
                &state.synteny_metrics_by_seq,
            );

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
        attach_rich_window_group_and_transition_metrics_to_attributes(
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
    let registry = AttributeRegistry::load_default().map_err(|err| {
        error::Error::Generic(format!("failed to load attribute registry: {err}"))
    })?;
    let mut attribute_docs = Vec::new();
    let mut keys = Vec::new();

    for feature in features {
        if let Some(attrs) = &feature.attributes {
            for attr in attrs {
                keys.push(attr.key.clone());
                let overrides =
                    crate::index::es::models::attribute_builder::feature_attribute_overrides(attr);
                attribute_docs.push(build_attribute_document(attr, Some(&overrides)));
            }
        }
    }

    let missing = registry.find_unmapped_keys(keys);
    if !missing.is_empty() {
        return Err(error::Error::Generic(format!(
            "attribute registry guard failed: unregistered attributes: {}",
            missing.join(", ")
        )));
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
    ensure_import_indices(&cfg.es)?;
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
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::index::es::models::nested_documents::NestedAttribute;

    #[test]
    fn attach_busco_category_counts_keeps_sequence_counts_from_parse_without_double_counting() {
        let mut state = ImportState::new("asm-1".to_string(), "tax-1".to_string());

        state
            .busco_counts
            .add_to_sequence("seq-1", "diptera_odb12", "complete");
        state.busco_id_tracker.record(
            "BUSCO_00001",
            "seq-1",
            vec!["win-1".to_string()],
            "Complete",
            "diptera_odb12",
        );

        attach_busco_category_counts(&mut state).unwrap();

        let seq_counts = state
            .busco_counts
            .seq_counts
            .get("seq-1")
            .and_then(|lineages| lineages.get("diptera_odb12"))
            .unwrap();

        assert_eq!(seq_counts.get("complete"), Some(&1));
        assert_eq!(seq_counts.get("single_copy"), None);
        assert_eq!(seq_counts.get("duplicated"), None);
        assert_eq!(
            state.busco_counts.window_counts["win-1"]["diptera_odb12"]["complete"],
            1
        );
    }

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
                group_counts: vec![("group-A".to_string(), 12), ("group-B".to_string(), 8)],
                top_transitions: vec![("group-A->group-B".to_string(), 3)],
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
            group_counts: vec![("group-A".to_string(), 1)],
            top_transitions: vec![],
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
                group_counts: vec![("group-B".to_string(), 2), ("group-A".to_string(), 1)],
                top_transitions: vec![("group-A->group-B".to_string(), 1)],
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
                group_counts: vec![("group-A".to_string(), 12), ("group-B".to_string(), 8)],
                top_transitions: vec![("group-A->group-B".to_string(), 2)],
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
                group_counts: vec![("group-A".to_string(), 12), ("group-B".to_string(), 8)],
                top_transitions: vec![("group-A->group-B".to_string(), 3)],
            },
        )]);

        attach_synteny_metrics_to_attributes(&mut attrs, "seq-1", &metrics_by_seq);

        assert!(attrs.iter().any(|attr| attr.key == "total_loci"));
        assert!(attrs.iter().any(|attr| attr.key == "majority_group_id"));
        assert!(attrs.iter().any(|attr| attr.key == "filtered_gini_score"));
        assert!(attrs.iter().all(|attr| attr.key != "normalised_gini_score"));
    }

    #[test]
    fn rich_group_and_transition_metrics_are_attached_separately() {
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
                group_counts: vec![("group-A".to_string(), 12), ("group-B".to_string(), 8)],
                top_transitions: vec![
                    ("group-A->group-B".to_string(), 3),
                    ("group-B->group-A".to_string(), 2),
                ],
            },
        )]);

        attach_rich_group_and_transition_metrics_to_attributes(
            &mut attrs,
            "seq-1",
            &metrics_by_seq,
        );

        let group_counts_attr = attrs
            .iter()
            .find(|attr| attr.key == "group_counts")
            .unwrap();
        let top_transitions_attr = attrs
            .iter()
            .find(|attr| attr.key == "top_transitions")
            .unwrap();

        assert!(group_counts_attr
            .flattened_value
            .as_ref()
            .is_some_and(|value| value.is_object()));
        assert!(group_counts_attr
            .flattened_value
            .as_ref()
            .and_then(|value| value.get("values"))
            .is_some());
        assert!(top_transitions_attr
            .flattened_value
            .as_ref()
            .is_some_and(|value| value.is_object()));
        assert!(top_transitions_attr
            .flattened_value
            .as_ref()
            .and_then(|value| value.get("values"))
            .is_some());
        assert!(attrs.iter().any(|attr| attr.key == "group_counts"));
        assert!(attrs.iter().any(|attr| attr.key == "top_transitions"));
        assert!(attrs.iter().all(|attr| attr.key != "normalised_gini_score"));
        assert!(attrs.iter().any(|attr| attr.key == "majority_group_id"));
    }

    #[test]
    fn set_synteny_loci_populates_top_transitions_before_metrics_are_derived() {
        let mut block_set = crate::parse::busco::SyntenyBlockSet::new(
            "lineage".to_string(),
            "seq-1".to_string(),
            "asm".to_string(),
            "tax".to_string(),
        );

        for idx in 1..=4 {
            block_set.add_locus_to_block(
                "group-A",
                crate::parse::busco::BuscoFeature {
                    id: format!("busco-a-{idx}"),
                    status: "Complete".to_string(),
                    score: 1.0,
                    sequence: "seq-1".to_string(),
                    start: ((idx - 1) * 10) + 1,
                    end: idx * 10,
                    strand: 1,
                    length: 10,
                },
            );
        }
        for idx in 1..=4 {
            block_set.add_locus_to_block(
                "group-B",
                crate::parse::busco::BuscoFeature {
                    id: format!("busco-b-{idx}"),
                    status: "Complete".to_string(),
                    score: 1.0,
                    sequence: "seq-1".to_string(),
                    start: 41 + ((idx - 1) * 10),
                    end: 50 + ((idx - 1) * 10),
                    strand: 1,
                    length: 10,
                },
            );
        }

        block_set.set_synteny_loci();
        assert!(block_set.transitions.is_some());
        let transitions = block_set.transitions.clone().unwrap();
        assert!(!transitions.is_empty());
        assert!(transitions
            .iter()
            .any(|(key, count)| key == "group-A->group-B" && *count == 1));

        block_set.set_metrics(block_set.group_model_count());
        let metrics = block_set
            .get_metrics()
            .expect("metrics should be available");
        assert!(!metrics.top_transitions.is_empty());
    }

    #[test]
    fn group_model_triggers_synteny_metrics_without_alg_count() {
        let mut block_set = crate::parse::busco::SyntenyBlockSet::new(
            "lineage".to_string(),
            "seq-1".to_string(),
            "asm".to_string(),
            "tax".to_string(),
        );

        block_set.add_locus_to_block(
            "group-A",
            crate::parse::busco::BuscoFeature {
                id: "busco-1".to_string(),
                status: "Complete".to_string(),
                score: 1.0,
                sequence: "seq-1".to_string(),
                start: 1,
                end: 10,
                strand: 1,
                length: 10,
            },
        );
        block_set.add_locus_to_block(
            "group-B",
            crate::parse::busco::BuscoFeature {
                id: "busco-2".to_string(),
                status: "Complete".to_string(),
                score: 1.0,
                sequence: "seq-1".to_string(),
                start: 11,
                end: 20,
                strand: 1,
                length: 10,
            },
        );

        assert!(block_set.has_group_model());
        assert_eq!(block_set.group_model_count(), 2);

        block_set.set_metrics(block_set.group_model_count());
        let metrics = block_set
            .get_metrics()
            .expect("group metrics should be set");
        assert_eq!(metrics.distinct_group_count, 2);
        assert!(!metrics.group_counts.is_empty());
    }
}
