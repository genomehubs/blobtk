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
    attributes::SyntenyIndexMode, parse_busco_files, BuscoFileConfig, MultiBuscoConfig,
};
use crate::parse::sequence_report;

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
            // attach block set metrics if available
            if let Some(metrics) = state.synteny_metrics_by_seq.get(seq_id) {
                let block_set_attrs = metrics.to_nested_attribute_docs();
                attrs.extend(block_set_attrs);
            }

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

    // Attach tallied busco counts to windows
    let mut window_docs_final = window_docs;
    for (window_id, window_doc) in window_docs_final.iter_mut() {
        if let Some(lineage_counts) = state.busco_counts.window_counts.get(window_id) {
            let mut attrs = window_doc.attributes.take().unwrap_or_default();

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

            window_doc.attributes = Some(attrs);
        }
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
                let overrides = crate::index::es::models::attribute_builder::feature_attribute_overrides(attr);
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
