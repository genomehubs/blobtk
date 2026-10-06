//!
//! Invoked by calling:
//! `blobtk import <args>`

// use crate::index::es::config;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::attribute_registry::AttributeRegistry;
use crate::config::legacy::normalize_legacy_import_config;
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
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

pub mod state;
use state::ImportState;

#[derive(Clone, Deserialize, Serialize, Debug, Default)]
pub struct HubConfig {
    pub name: String,
    pub release: String,
    pub taxonomy: String,
}

#[derive(Clone, Deserialize, Serialize, Debug, Default)]
pub struct EsConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub hub: HubConfig,
}

#[derive(Clone, Deserialize, Serialize, Debug, Default)]
pub struct SequenceReportImportConfig {
    pub accession: String,
    #[serde(default)]
    pub taxon_id: String,
    #[serde(default)]
    pub ancestors: Vec<String>,
    #[serde(default)]
    pub path: Option<std::path::PathBuf>,
    #[serde(default)]
    pub local_path: Option<std::path::PathBuf>,
}

#[derive(Clone, Deserialize, Serialize, Debug, Default)]
pub struct AssemblyImportConfig {
    pub accession: String,
    #[serde(default)]
    pub taxon_id: Option<String>,
    #[serde(default)]
    pub ancestors: Vec<String>,
    #[serde(default)]
    pub lineage: Vec<TaxonLineageEntry>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct ImportOptions {
    #[serde(default, alias = "clear_feature_index")]
    pub clear_feature_data: bool,
    #[serde(default)]
    pub entity_types: Option<Vec<String>>, // ["sequence", "window", "busco"]
    #[serde(default)]
    pub busco_tallies: Option<BuscoTalliesConfig>,
    #[serde(default)]
    pub synteny_index: Option<SyntenyIndexMode>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BuscoTalliesConfig {
    pub lineages: Vec<String>,
    pub assembly_counts_output: Option<std::path::PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaxonLineageEntry {
    pub taxon_id: String,
    pub rank: String,
    #[serde(default)]
    pub scientific_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct BatchTemplateRef {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct BatchMember {
    pub accession: String,
    #[serde(default)]
    pub taxon_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Default)]
pub struct BatchDefaults {
    #[serde(default)]
    pub template: Option<BatchTemplateRef>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct BatchConfig {
    pub id: String,
    pub rank: String,
    #[serde(default)]
    pub taxon_id: Option<String>,
    #[serde(default)]
    pub taxon_name: Option<String>,
    #[serde(default)]
    pub template: Option<BatchTemplateRef>,
    #[serde(default)]
    pub members: Vec<BatchMember>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct BatchManifest {
    pub schema_version: u32,
    pub taxonomy: String,
    #[serde(default)]
    pub generated_at: Option<String>,
    pub min_members: usize,
    #[serde(default)]
    pub defaults: BatchDefaults,
    #[serde(rename = "batches", alias = "batches")]
    pub batches: Vec<BatchConfig>,
}

fn validate_batch_manifest(manifest: &BatchManifest) -> Result<(), anyhow::Error> {
    if manifest.min_members == 0 {
        return Err(anyhow::anyhow!(
            "batch min_members must be greater than zero"
        ));
    }

    for batch in &manifest.batches {
        if batch.id.trim().is_empty() {
            return Err(anyhow::anyhow!("batch id cannot be empty"));
        }
        if batch.rank.trim().is_empty() {
            return Err(anyhow::anyhow!("batch {} is missing a rank", batch.id));
        }
        if batch.members.is_empty() {
            return Err(anyhow::anyhow!(
                "batch {} has no members; the import list must not be empty",
                batch.id
            ));
        }
        if batch.members.len() < manifest.min_members {
            return Err(anyhow::anyhow!(
                "batch {} has {} members, below the minimum of {}",
                batch.id,
                batch.members.len(),
                manifest.min_members
            ));
        }
        for member in &batch.members {
            if member.accession.trim().is_empty() {
                return Err(anyhow::anyhow!(
                    "batch {} contains a member with an empty accession in members",
                    batch.id
                ));
            }
        }
    }

    Ok(())
}

// fn resolve_batch_template_path(
//     batch: &BatchConfig,
//     defaults: &BatchDefaults,
// ) -> Result<PathBuf, anyhow::Error> {
//     let raw_path = batch
//         .template
//         .as_ref()
//         .or(defaults.template.as_ref())
//         .map(|template| template.path.clone())
//         .ok_or_else(|| anyhow::anyhow!("no batch template configured for batch {}", batch.id))?;

//     if raw_path.is_absolute() {
//         Ok(raw_path)
//     } else {
//         std::env::current_dir()
//             .map(|cwd| cwd.join(raw_path))
//             .map_err(|err| {
//                 anyhow::anyhow!("failed to resolve batch template for {}: {err}", batch.id)
//             })
//     }
// }

fn resolve_batch_template_path_for_manifest(
    manifest_path: &PathBuf,
    batch: &BatchConfig,
    defaults: &BatchDefaults,
) -> Result<PathBuf, anyhow::Error> {
    let raw_path = batch
        .template
        .as_ref()
        .or(defaults.template.as_ref())
        .map(|template| template.path.clone())
        .ok_or_else(|| anyhow::anyhow!("no batch template configured for batch {}", batch.id))?;

    if raw_path.is_absolute() {
        Ok(raw_path)
    } else {
        let base_dir = manifest_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        Ok(base_dir.join(raw_path))
    }
}

// fn expand_batch_member_config(
//     batch: &BatchConfig,
//     member: &BatchMember,
//     defaults: &BatchDefaults,
//     remote_root: &str,
//     local_root: &str,
// ) -> Result<ImportConfig, anyhow::Error> {
//     let template_path = resolve_batch_template_path(batch, defaults)?;
//     let template_text = std::fs::read_to_string(&template_path)?;
//     let taxon_id = member
//         .taxon_id
//         .as_deref()
//         .or_else(|| batch.taxon_id.as_deref())
//         .unwrap_or("");
//     let expanded = template_text
//         .replace("{ACCESSION}", &member.accession)
//         .replace("{TAXON_ID}", taxon_id)
//         .replace("{REMOTE_ROOT}", remote_root)
//         .replace("{LOCAL_ROOT}", local_root)
//         .replace("{BATCH_ID}", &batch.id)
//         .replace("{RANK}", &batch.rank)
//         .replace("{TAXON_NAME}", batch.taxon_name.as_deref().unwrap_or(""));

//     let config: ImportConfig = serde_yaml::from_str(&expanded)?;
//     Ok(config)
// }

fn expand_batch_member_config_for_manifest(
    manifest_path: &PathBuf,
    batch: &BatchConfig,
    member: &BatchMember,
    defaults: &BatchDefaults,
    remote_root: &str,
    local_root: &str,
) -> Result<ImportConfig, anyhow::Error> {
    let template_path = resolve_batch_template_path_for_manifest(manifest_path, batch, defaults)?;
    let template_text = std::fs::read_to_string(&template_path)?;
    let taxon_id = member
        .taxon_id
        .as_deref()
        .or_else(|| batch.taxon_id.as_deref())
        .unwrap_or("");
    let expanded = template_text
        .replace("{ACCESSION}", &member.accession)
        .replace("{TAXON_ID}", taxon_id)
        .replace("{REMOTE_ROOT}", remote_root)
        .replace("{LOCAL_ROOT}", local_root)
        .replace("{BATCH_ID}", &batch.id)
        .replace("{RANK}", &batch.rank)
        .replace("{TAXON_NAME}", batch.taxon_name.as_deref().unwrap_or(""));

    let config: ImportConfig = serde_yaml::from_str(&expanded)?;
    Ok(config)
}

fn expand_batch_member_configs_for_batch(
    manifest_path: &PathBuf,
    batch: &BatchConfig,
    defaults: &BatchDefaults,
    members: &[BatchMember],
    remote_root: &str,
    local_root: &str,
) -> Result<Vec<ImportConfig>, anyhow::Error> {
    let mut configs = Vec::new();
    for member in members {
        let config = expand_batch_member_config_for_manifest(
            manifest_path,
            batch,
            member,
            defaults,
            remote_root,
            local_root,
        )?;
        configs.push(config);
    }

    Ok(configs)
}

fn expand_batch_member_configs_for_manifest(
    manifest_path: &PathBuf,
    batch_id: &str,
    remote_root: &str,
    local_root: &str,
) -> Result<Vec<ImportConfig>, anyhow::Error> {
    let manifest = load_batch_manifest(manifest_path)?;
    let batch = manifest
        .batches
        .iter()
        .find(|candidate| candidate.id == batch_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "batch {} not found in manifest {}",
                batch_id,
                manifest_path.display()
            )
        })?;

    expand_batch_member_configs_for_batch(
        manifest_path,
        batch,
        &manifest.defaults,
        &batch.members,
        remote_root,
        local_root,
    )
}

fn load_batch_manifest(path: &PathBuf) -> Result<BatchManifest, anyhow::Error> {
    let text = std::fs::read_to_string(path)?;
    let manifest: BatchManifest = serde_yaml::from_str(&text)?;
    validate_batch_manifest(&manifest)?;
    Ok(manifest)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum BatchLevel {
    Family,
    Order,
    Class,
    Phylum,
    Kingdom,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum MetricFamily {
    Gc,
    Repeat,
    Satellite,
    Coverage,
    Gaps,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum CoordinateSystem {
    PhysicalWindow,
    ProportionalBin,
    SequencePosition,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct BatchResolution {
    pub batch_id: String,
    pub level: BatchLevel,
    pub members: Vec<String>,
    pub metric_family: MetricFamily,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct NormalizationSummary {
    pub batch_size: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub median: Option<f64>,
    pub mad: Option<f64>,
    pub version: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct NormalizationBaselineCache {
    entries: HashMap<String, NormalizationSummary>,
}

impl NormalizationBaselineCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        batch_id: &str,
        metric_family: &MetricFamily,
        coordinate_system: &CoordinateSystem,
        version: &str,
        summary: NormalizationSummary,
    ) {
        let key = format!(
            "{}|{}|{}|{}",
            batch_id,
            metric_family_name(metric_family),
            coordinate_system_name(coordinate_system),
            version
        );
        self.entries.insert(key, summary);
    }

    pub fn get(
        &self,
        batch_id: &str,
        metric_family: &MetricFamily,
        coordinate_system: &CoordinateSystem,
        version: &str,
    ) -> Option<&NormalizationSummary> {
        let key = format!(
            "{}|{}|{}|{}",
            batch_id,
            metric_family_name(metric_family),
            coordinate_system_name(coordinate_system),
            version
        );
        self.entries.get(&key)
    }
}

fn metric_family_name(metric_family: &MetricFamily) -> &'static str {
    match metric_family {
        MetricFamily::Gc => "gc",
        MetricFamily::Repeat => "repeat",
        MetricFamily::Satellite => "satellite",
        MetricFamily::Coverage => "coverage",
        MetricFamily::Gaps => "gaps",
    }
}

fn coordinate_system_name(coordinate_system: &CoordinateSystem) -> &'static str {
    match coordinate_system {
        CoordinateSystem::PhysicalWindow => "physical_window",
        CoordinateSystem::ProportionalBin => "proportional_bin",
        CoordinateSystem::SequencePosition => "sequence_position",
    }
}

fn resolve_taxon_batch_for_metric(
    lineage: &[TaxonLineageEntry],
    batch_members: &HashMap<String, Vec<String>>,
    metric_family: MetricFamily,
    min_size: usize,
) -> Result<BatchResolution, anyhow::Error> {
    let batch_levels = [
        ("family", BatchLevel::Family),
        ("order", BatchLevel::Order),
        ("class", BatchLevel::Class),
        ("phylum", BatchLevel::Phylum),
        ("kingdom", BatchLevel::Kingdom),
    ];

    for (rank, level) in batch_levels {
        let taxon_id = lineage
            .iter()
            .find(|entry| entry.rank.eq_ignore_ascii_case(rank))
            .map(|entry| entry.taxon_id.clone());

        let Some(taxon_id) = taxon_id else {
            continue;
        };

        let batch_key = format!("{rank}:{taxon_id}");
        let members = batch_members.get(&batch_key).cloned().unwrap_or_default();

        if members.len() >= min_size {
            return Ok(BatchResolution {
                batch_id: format!(
                    "{}:{}:{}",
                    level_name(&level),
                    taxon_id,
                    metric_family_name(&metric_family)
                ),
                level,
                members,
                metric_family,
            });
        }
    }

    Err(anyhow::anyhow!(
        "no stable batch for metric {} with minimum size {} in lineage {:?}",
        metric_family_name(&metric_family),
        min_size,
        lineage
    ))
}

fn level_name(level: &BatchLevel) -> &'static str {
    match level {
        BatchLevel::Family => "family",
        BatchLevel::Order => "order",
        BatchLevel::Class => "class",
        BatchLevel::Phylum => "phylum",
        BatchLevel::Kingdom => "kingdom",
    }
}

fn precompute_normalization_baseline(
    lineage: &[TaxonLineageEntry],
    batch_members: &HashMap<String, Vec<String>>,
    metric_family: MetricFamily,
    coordinate_system: CoordinateSystem,
    version: &str,
    observed_values: &[f64],
    min_size: usize,
) -> Result<(BatchResolution, NormalizationSummary), anyhow::Error> {
    let resolution =
        resolve_taxon_batch_for_metric(lineage, batch_members, metric_family, min_size)?;
    let batch_size = resolution.members.len().max(1);
    let mean = if observed_values.is_empty() {
        0.0
    } else {
        observed_values.iter().sum::<f64>() / observed_values.len() as f64
    };
    let variance = if observed_values.len() <= 1 {
        0.0
    } else {
        let diff = observed_values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>();
        diff / (observed_values.len() as f64 - 1.0)
    };
    let std_dev = variance.sqrt();
    let median = {
        let mut sorted = observed_values.to_vec();
        if !sorted.is_empty() {
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mid = sorted.len() / 2;
            if sorted.len() % 2 == 0 {
                Some((sorted[mid - 1] + sorted[mid]) / 2.0)
            } else {
                Some(sorted[mid])
            }
        } else {
            None
        }
    };
    let mad = median.map(|median_value| {
        if observed_values.is_empty() {
            0.0
        } else {
            let mut deviations: Vec<f64> = observed_values
                .iter()
                .map(|value| (value - median_value).abs())
                .collect();
            deviations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mid = deviations.len() / 2;
            if deviations.len() % 2 == 0 {
                (deviations[mid - 1] + deviations[mid]) / 2.0
            } else {
                deviations[mid]
            }
        }
    });

    let summary = NormalizationSummary {
        batch_size,
        mean,
        std_dev,
        median,
        mad,
        version: version.to_string(),
    };
    Ok((resolution, summary))
}

#[derive(Clone, Deserialize, Serialize, Debug, Default)]
pub struct ImportConfig {
    pub assembly: AssemblyImportConfig,
    pub es: EsConfig,
    pub sequence_report: SequenceReportImportConfig,
    pub bed: MultiBedConfig,
    pub busco: MultiBuscoConfig,
    pub import: Option<ImportOptions>,
}

static ASSEMBLY_TAXON_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
static ASSEMBLY_ANCESTORS_CACHE: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
static ASSEMBLY_LINEAGE_CACHE: OnceLock<Mutex<HashMap<String, Vec<TaxonLineageEntry>>>> =
    OnceLock::new();

fn lookup_assembly_lineage(
    accession: &str,
    base_url: &str,
) -> Result<Vec<TaxonLineageEntry>, anyhow::Error> {
    let cache_key = format!("{}|{}", base_url, accession);
    {
        let cache = ASSEMBLY_LINEAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(lineage) = cache.lock().unwrap().get(&cache_key).cloned() {
            return Ok(lineage);
        }
    }

    let response = lookup_assembly_record(accession, base_url)?;
    let lineage = response["records"]
        .as_array()
        .and_then(|records| records.first())
        .and_then(|record| record.get("record"))
        .and_then(|record| record.get("lineage"))
        .and_then(|lineage| lineage.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|entry| {
                    let taxon_id = entry.get("taxon_id")?.as_str()?.to_string();
                    let rank = entry.get("rank")?.as_str()?.to_string();
                    let scientific_name = entry
                        .get("scientific_name")
                        .and_then(|value| value.as_str())
                        .map(str::to_string);
                    Some(TaxonLineageEntry {
                        taxon_id,
                        rank,
                        scientific_name,
                    })
                })
                .collect::<Vec<TaxonLineageEntry>>()
        })
        .unwrap_or_default();

    let cache = ASSEMBLY_LINEAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    cache.lock().unwrap().insert(cache_key, lineage.clone());
    Ok(lineage)
}

fn lookup_assembly_record(
    accession: &str,
    base_url: &str,
) -> Result<serde_json::Value, anyhow::Error> {
    let url =
        format!("{base_url}/api/v2/record?recordId={accession}&result=assembly&groups=lineage");
    let body = reqwest::blocking::get(&url)?.error_for_status()?.text()?;
    Ok(serde_json::from_str(&body)?)
}

fn lookup_taxon_id_for_assembly(
    accession: &str,
    taxonomy: &str,
    base_url: &str,
) -> Result<String, anyhow::Error> {
    let cache_key = format!("{}|{}|{}", base_url, taxonomy, accession);
    {
        let cache = ASSEMBLY_TAXON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(taxon_id) = cache.lock().unwrap().get(&cache_key).cloned() {
            return Ok(taxon_id);
        }
    }

    let response = lookup_assembly_record(accession, base_url)?;
    let taxon_id = response["records"]
        .as_array()
        .and_then(|records| records.first())
        .and_then(|record| record.get("record"))
        .and_then(|record| record.get("taxon_id"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("assembly taxon_id not found for assembly {}", accession))?
        .to_string();

    let cache = ASSEMBLY_TAXON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    cache.lock().unwrap().insert(cache_key, taxon_id.clone());
    Ok(taxon_id)
}

fn lookup_ancestor_taxon_ids_for_assembly(
    accession: &str,
    base_url: &str,
) -> Result<Vec<String>, anyhow::Error> {
    let cache_key = format!("{}|{}", base_url, accession);
    {
        let cache = ASSEMBLY_ANCESTORS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(ancestors) = cache.lock().unwrap().get(&cache_key).cloned() {
            return Ok(ancestors);
        }
    }

    let response = lookup_assembly_record(accession, base_url)?;
    let ancestors = response["records"]
        .as_array()
        .and_then(|records| records.first())
        .and_then(|record| record.get("record"))
        .and_then(|record| record.get("lineage"))
        .and_then(|lineage| lineage.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|entry| entry.get("taxon_id").and_then(|value| value.as_str()))
                .map(|value| value.to_string())
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();

    let cache = ASSEMBLY_ANCESTORS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    cache.lock().unwrap().insert(cache_key, ancestors.clone());
    Ok(ancestors)
}

fn resolve_assembly_taxon_id_with_base_url(
    cfg: &mut ImportConfig,
    base_url: &str,
) -> Result<(), anyhow::Error> {
    let taxon_id = match cfg.assembly.taxon_id.clone() {
        Some(taxon_id) => taxon_id,
        None => {
            let taxonomy = cfg.es.hub.taxonomy.trim();
            lookup_taxon_id_for_assembly(&cfg.assembly.accession, taxonomy, base_url)?
        }
    };

    let ancestor_taxon_ids =
        lookup_ancestor_taxon_ids_for_assembly(&cfg.assembly.accession, base_url)?;
    let lineage = lookup_assembly_lineage(&cfg.assembly.accession, base_url)?;

    cfg.assembly.taxon_id = Some(taxon_id.clone());
    cfg.assembly.ancestors = ancestor_taxon_ids.clone();
    cfg.assembly.lineage = lineage.clone();
    cfg.sequence_report.taxon_id = taxon_id.clone();
    cfg.sequence_report.ancestors = ancestor_taxon_ids.clone();
    cfg.bed.taxon_id = taxon_id.clone();
    cfg.bed.ancestors = ancestor_taxon_ids.clone();
    cfg.busco.taxon_id = taxon_id.clone();
    cfg.busco.ancestors = ancestor_taxon_ids.clone();
    Ok(())
}

fn resolve_assembly_taxon_id(cfg: &mut ImportConfig) -> Result<(), anyhow::Error> {
    resolve_assembly_taxon_id_with_base_url(cfg, "https://goat.genomehubs.org")
}

fn expand_busco_tables(cfg: &mut ImportConfig) {
    let accession = cfg.assembly.accession.clone();
    let taxon = cfg.assembly.taxon_id.clone().unwrap_or_default();
    let mut expanded: Vec<BuscoFileConfig> = Vec::new();

    if let Some(existing) = &cfg.busco.files {
        for file in existing.iter() {
            let p = file
                .path
                .to_string_lossy()
                .to_string()
                .replace("{ACCESSION}", &accession)
                .replace("{TAXON}", &taxon)
                .replace("{TAXON_ID}", &taxon)
                .replace("{LINEAGE}", &file.lineage);
            let local_p = file.local_path.as_ref().map(|s| {
                s.to_string_lossy()
                    .to_string()
                    .replace("{ACCESSION}", &accession)
                    .replace("{TAXON}", &taxon)
                    .replace("{TAXON_ID}", &taxon)
                    .replace("{LINEAGE}", &file.lineage)
            });
            expanded.push(BuscoFileConfig {
                path: PathBuf::from(p),
                local_path: local_p.as_ref().map(|s| PathBuf::from(s)),
                lineage: file.lineage.clone(),
                taxon_id: file.taxon_id.clone(),
                accession: file.accession.clone(),
                ancestors: file.ancestors.clone(),
            });
        }
        cfg.busco.files = Some(expanded);
        return;
    }

    if let Some(tables) = &cfg.busco.tables {
        for table in tables.iter() {
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
                        .replace("{TAXON}", &taxon)
                        .replace("{TAXON_ID}", &taxon);
                    let local_p = local_path_str.as_ref().map(|s| {
                        s.replace("{ACCESSION}", &accession)
                            .replace("{LINEAGE}", lineage)
                            .replace("{TAXON}", &taxon)
                            .replace("{TAXON_ID}", &taxon)
                    });
                    expanded.push(BuscoFileConfig {
                        path: PathBuf::from(p),
                        local_path: local_p.as_ref().map(|s| PathBuf::from(s)),
                        lineage: lineage.clone(),
                        taxon_id: taxon.clone(),
                        accession: accession.clone(),
                        ancestors: cfg.assembly.ancestors.clone(),
                    });
                }
            } else {
                let p = path_str
                    .replace("{ACCESSION}", &accession)
                    .replace("{TAXON}", &taxon)
                    .replace("{TAXON_ID}", &taxon);
                let local_p = local_path_str.as_ref().map(|s| {
                    s.replace("{ACCESSION}", &accession)
                        .replace("{TAXON}", &taxon)
                        .replace("{TAXON_ID}", &taxon)
                });
                expanded.push(BuscoFileConfig {
                    path: PathBuf::from(p),
                    local_path: local_p.as_ref().map(|s| PathBuf::from(s)),
                    lineage: String::new(),
                    taxon_id: taxon.clone(),
                    accession: accession.clone(),
                    ancestors: cfg.assembly.ancestors.clone(),
                });
            }
        }
    }
    cfg.busco.files = Some(expanded);
}

fn expand_placeholders(cfg: &mut ImportConfig) {
    let accession = cfg.assembly.accession.clone();
    let taxon = cfg.assembly.taxon_id.clone().unwrap_or_default();

    let s = cfg
        .sequence_report
        .path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());
    if let Some(s) = s {
        let s = s
            .replace("{ACCESSION}", &accession)
            .replace("{TAXON}", &taxon)
            .replace("{TAXON_ID}", &taxon);
        cfg.sequence_report.path = Some(std::path::PathBuf::from(s));
    }

    let s = cfg
        .sequence_report
        .local_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());
    if let Some(s) = s {
        let s = s
            .replace("{ACCESSION}", &accession)
            .replace("{TAXON}", &taxon)
            .replace("{TAXON_ID}", &taxon);
        cfg.sequence_report.local_path = Some(std::path::PathBuf::from(s));
    }

    for bed in cfg.bed.bed_configs.iter_mut() {
        let s = bed.path.to_string_lossy().to_string();
        let s = s
            .replace("{ACCESSION}", &accession)
            .replace("{TAXON}", &taxon)
            .replace("{TAXON_ID}", &taxon);
        bed.path = std::path::PathBuf::from(s);
        if let Some(local_path) = &bed.local_path {
            let s = local_path.to_string_lossy().to_string();
            let s = s
                .replace("{ACCESSION}", &accession)
                .replace("{TAXON}", &taxon)
                .replace("{TAXON_ID}", &taxon);
            bed.local_path = Some(std::path::PathBuf::from(s));
        }
    }

    if let Some(algs) = &mut cfg.busco.algs {
        for alg in algs.iter_mut() {
            let path = alg.path.to_string_lossy().to_string();
            alg.path = PathBuf::from(
                path.replace("{ACCESSION}", &accession)
                    .replace("{TAXON}", &taxon)
                    .replace("{TAXON_ID}", &taxon)
                    .replace("{LINEAGE}", &alg.lineage),
            );
            if let Some(local_path) = &alg.local_path {
                let path = local_path.to_string_lossy().to_string();
                alg.local_path = Some(PathBuf::from(
                    path.replace("{ACCESSION}", &accession)
                        .replace("{TAXON}", &taxon)
                        .replace("{TAXON_ID}", &taxon)
                        .replace("{LINEAGE}", &alg.lineage),
                ));
            }
        }
    }

    expand_busco_tables(cfg);

    let s = cfg
        .sequence_report
        .local_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());
    if let Some(s) = s {
        let s = s
            .replace("{ACCESSION}", &accession)
            .replace("{TAXON}", &taxon)
            .replace("{TAXON_ID}", &taxon);
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

fn attach_bed_summary_metrics_to_sequences(
    state: &mut ImportState,
    bed_cfg: &MultiBedConfig,
) -> Result<(), error::Error> {
    let mut sequence_summary_attrs: HashMap<String, Vec<NestedAttribute>> = HashMap::new();

    for bed_config in &bed_cfg.bed_configs {
        let per_seq_buffers = crate::parse::bed::read_bed_file(bed_config)?;
        let assembly_stats: Vec<(f64, f64)> = bed_config
            .value_columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                let values: Vec<f64> = per_seq_buffers
                    .values()
                    .flat_map(|buffer| buffer.iter())
                    .flat_map(|feature| feature.values.get(index).copied())
                    .collect();
                column
                    .normalisation
                    .as_ref()
                    .map_or((0.0, 0.0), |normalisation| {
                        crate::parse::bed::normalisation_stats_for_config(&values, normalisation)
                    })
            })
            .collect();

        for (seq_id, buffer) in per_seq_buffers {
            let values_by_column: Vec<Vec<f64>> = bed_config
                .value_columns
                .iter()
                .enumerate()
                .map(|(index, column)| {
                    buffer
                        .iter()
                        .flat_map(|feature| feature.values.get(index).copied())
                        .collect::<Vec<_>>()
                })
                .collect();

            let seq_attrs = sequence_summary_attrs.entry(seq_id.clone()).or_default();
            for (index, column) in bed_config.value_columns.iter().enumerate() {
                let values = &values_by_column[index];
                if values.is_empty() {
                    continue;
                }

                let primary_summary = column
                    .summary_functions
                    .first()
                    .cloned()
                    .unwrap_or(crate::parse::bed::SummaryFunction::Mean);
                let summary_value = primary_summary.compute(values);
                if summary_value.is_finite() {
                    seq_attrs.push(NestedAttribute {
                        key: column.label.clone(),
                        half_float_value: Some(summary_value as f32),
                        ..Default::default()
                    });
                }

                if let Some(normalisation) = &column.normalisation {
                    let transformed_key = column
                        .transformed_name()
                        .unwrap_or_else(|| format!("{}_zscore", column.label));
                    let scope_stats = match normalisation.scope {
                        crate::parse::bed::NormalisationScope::Assembly => {
                            assembly_stats.get(index).copied().unwrap_or((0.0, 0.0))
                        }
                        crate::parse::bed::NormalisationScope::Sequence => {
                            let seq_values: Vec<f64> = values.clone();
                            crate::parse::bed::normalisation_stats_for_config(
                                &seq_values,
                                normalisation,
                            )
                        }
                    };
                    let transformed_value = crate::parse::bed::apply_normalisation(
                        summary_value,
                        normalisation,
                        scope_stats,
                    );
                    seq_attrs.push(NestedAttribute {
                        key: transformed_key,
                        half_float_value: Some(transformed_value as f32),
                        ..Default::default()
                    });
                }
            }
        }
    }

    for (seq_id, attrs) in sequence_summary_attrs {
        if let Some(seq_doc) = state.sequences.get_mut(&seq_id) {
            let mut existing = seq_doc.attributes.take().unwrap_or_default();
            existing.extend(attrs);
            seq_doc.attributes = Some(existing);
        }
    }

    Ok(())
}

fn validate_bed_sequence_coverage(
    sequence_features: &HashMap<String, FeatureDocument>,
    bed_cfg: &MultiBedConfig,
) -> Result<(), error::Error> {
    let report_ids: HashSet<_> = sequence_features.keys().cloned().collect();
    let mut bed_ids = HashSet::new();

    for bed_config in &bed_cfg.bed_configs {
        let per_seq_buffers = crate::parse::bed::read_bed_file(bed_config)?;
        for seq_id in per_seq_buffers.keys() {
            bed_ids.insert(seq_id.clone());
        }
    }

    if report_ids.is_empty() || bed_ids.is_empty() {
        return Ok(());
    }

    let missing_from_bed: Vec<_> = report_ids.difference(&bed_ids).cloned().collect();
    let missing_from_report: Vec<_> = bed_ids.difference(&report_ids).cloned().collect();

    if !missing_from_bed.is_empty() || !missing_from_report.is_empty() {
        let mut details = Vec::new();
        if !missing_from_bed.is_empty() {
            details.push(format!(
                "sequence report IDs missing from BED: {}",
                missing_from_bed
                    .iter()
                    .take(10)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !missing_from_report.is_empty() {
            details.push(format!(
                "BED IDs missing from sequence report: {}",
                missing_from_report
                    .iter()
                    .take(10)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        return Err(error::Error::Generic(format!(
            "BED and sequence report IDs do not match for assembly {}. {}",
            bed_cfg.accession,
            details.join("; ")
        )));
    }

    Ok(())
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

    let canonical_sequence_lengths: HashMap<String, usize> = state
        .sequences
        .iter()
        .map(|(seq_id, feature)| (seq_id.clone(), feature.sequence_length))
        .collect();
    let window_docs = parse_bed_files(bed_cfg, Some(&canonical_sequence_lengths))?;

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

fn clear_feature_data(es_cfg: &EsConfig) -> Result<(), error::Error> {
    let client = ElasticsearchClient::try_from(es_cfg)?;
    let feature_index = client.resolve_index_name("feature")?;

    if client.get_index_info(&feature_index).is_ok() {
        eprintln!("  Clearing existing feature index {}", feature_index);
        client.delete_index(&feature_index)?;
    }

    ensure_index_exists(
        es_cfg,
        "feature",
        crate::index::es::mappings::feature_index_mappings(),
    )?;

    let attributes_index = client.resolve_index_name("attributes")?;
    if client.get_index_info(&attributes_index).is_ok() {
        eprintln!(
            "  Clearing feature attribute definitions from {}",
            attributes_index
        );
        client.delete_by_query(
            &attributes_index,
            serde_json::json!({
                "query": {
                    "term": {
                        "group": "feature"
                    }
                }
            }),
        )?;
        client.refresh("attributes")?;
    }

    Ok(())
}

fn run_single_import_config(
    mut cfg: ImportConfig,
    staged_cfg: Option<&crate::config::schema::StagedImportConfig>,
) -> Result<(), anyhow::Error> {
    if cfg
        .import
        .as_ref()
        .is_some_and(|import_opts| import_opts.clear_feature_data)
    {
        clear_feature_data(&cfg.es)?;
    }

    resolve_assembly_taxon_id(&mut cfg)?;
    expand_placeholders(&mut cfg);
    if let Some(staged_cfg) = staged_cfg {
        crate::config::legacy::validate_legacy_runtime_matches_staged(&cfg, staged_cfg)?;
    }

    let assembly_id = cfg.assembly.accession.clone();
    let taxon_id = cfg.assembly.taxon_id.clone().unwrap_or_default();
    let mut import_state = ImportState::new(assembly_id, taxon_id.clone());
    import_state.lineage = cfg.assembly.lineage.clone();
    if !cfg.assembly.lineage.is_empty() {
        let mut batch_members: HashMap<String, Vec<String>> = HashMap::new();
        if cfg
            .assembly
            .lineage
            .iter()
            .any(|entry| entry.rank.eq_ignore_ascii_case("family"))
        {
            let family_id = cfg
                .assembly
                .lineage
                .iter()
                .find(|entry| entry.rank.eq_ignore_ascii_case("family"))
                .map(|entry| entry.taxon_id.clone());
            if let Some(family_id) = family_id {
                batch_members.insert(format!("family:{family_id}"), vec![taxon_id.clone()]);
            }
        }
        if let Ok((resolution, summary)) = precompute_normalization_baseline(
            &cfg.assembly.lineage,
            &batch_members,
            MetricFamily::Gc,
            CoordinateSystem::PhysicalWindow,
            "phase-2b",
            &[0.25, 0.5, 0.75],
            5,
        ) {
            import_state.normalization_cache.insert(
                &resolution.batch_id,
                &resolution.metric_family,
                &CoordinateSystem::PhysicalWindow,
                "phase-2b",
                summary,
            );
        }
    }
    ensure_import_indices(&cfg.es)?;
    restore_attribute_cache(&mut import_state, &cfg.es)?;

    eprintln!("Step 1: Parsing sequence report...");
    let sequence_report_cfg = cfg.sequence_report;
    let sequence_features = sequence_report::parse_sequence_report(sequence_report_cfg)?;
    import_state.sequences = sequence_features.clone();

    validate_bed_sequence_coverage(&sequence_features, &cfg.bed)?;

    let seq_vec: Vec<_> = import_state.sequences.values().cloned().collect();
    create_attribute_docs_from_features(&seq_vec, &mut import_state, &cfg.es, &cfg.import)?;

    attach_bed_summary_metrics_to_sequences(&mut import_state, &cfg.bed)?;

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

pub fn import(options: &crate::cli::ImportOptions) -> Result<(), anyhow::Error> {
    let config_path = &options.config;
    let yaml_text = std::fs::read_to_string(config_path)?;
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(&yaml_text)?;
    let clear_feature_data = options.clear_feature_data;

    let is_staged_config = yaml_value.get("sequence").is_some()
        || yaml_value.get("windowing").is_some()
        || yaml_value.get("annotations").is_some();

    if is_staged_config {
        let mut staged_cfg: crate::config::schema::StagedImportConfig =
            serde_yaml::from_str(&yaml_text)?;
        crate::config::normalize::normalize_staged_import_config(&mut staged_cfg);
        crate::config::legacy::expand_staged_placeholders(&mut staged_cfg);
        crate::config::legacy::validate_staged_import_config(&staged_cfg)?;
        let mut cfg = crate::config::legacy::staged_import_config_to_legacy_config(&staged_cfg);
        if clear_feature_data {
            cfg.import
                .get_or_insert_with(Default::default)
                .clear_feature_data = true;
        }
        resolve_assembly_taxon_id(&mut cfg)?;
        expand_placeholders(&mut cfg);
        run_single_import_config(cfg, Some(&staged_cfg))?;
        return Ok(());
    }

    let is_batch_manifest =
        yaml_value.get("batches").is_some() || yaml_value.get("batches").is_some();
    if is_batch_manifest {
        let manifest: BatchManifest = serde_yaml::from_str(&yaml_text)?;
        validate_batch_manifest(&manifest)?;

        let batch_id = options.batch.as_deref().unwrap_or_else(|| {
            manifest
                .batches
                .first()
                .map(|batch| batch.id.as_str())
                .unwrap_or("")
        });
        if batch_id.trim().is_empty() {
            return Err(anyhow::anyhow!("batch manifest has no selected batch"));
        }

        let batch = manifest
            .batches
            .iter()
            .find(|candidate| candidate.id == batch_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "batch {} not found in manifest {}",
                    batch_id,
                    config_path.display()
                )
            })?;

        let manifest_path = config_path.to_path_buf();
        let member_configs = expand_batch_member_configs_for_batch(
            &manifest_path,
            batch,
            &manifest.defaults,
            &batch.members,
            &options.remote_root,
            &options.local_root,
        )?;
        for mut cfg in member_configs {
            if clear_feature_data {
                cfg.import
                    .get_or_insert_with(Default::default)
                    .clear_feature_data = true;
            }
            resolve_assembly_taxon_id(&mut cfg)?;
            expand_placeholders(&mut cfg);
            let staged_cfg = normalize_legacy_import_config(&cfg);
            crate::config::legacy::validate_staged_import_config(&staged_cfg)?;
            run_single_import_config(cfg, Some(&staged_cfg))?;
        }
        return Ok(());
    }

    let mut cfg: ImportConfig = serde_yaml::from_str(&yaml_text)?;
    if clear_feature_data {
        cfg.import
            .get_or_insert_with(Default::default)
            .clear_feature_data = true;
    }
    resolve_assembly_taxon_id(&mut cfg)?;
    expand_placeholders(&mut cfg);
    let staged_cfg = normalize_legacy_import_config(&cfg);
    crate::config::legacy::validate_staged_import_config(&staged_cfg)?;
    run_single_import_config(cfg, Some(&staged_cfg))
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::index::es::models::nested_documents::NestedAttribute;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn import_options_allow_clear_feature_data_flag() {
        let from_yaml: ImportOptions = serde_yaml::from_str("clear_feature_data: true\n").unwrap();
        assert!(from_yaml.clear_feature_data);

        let from_alias: ImportOptions =
            serde_yaml::from_str("clear_feature_index: true\n").unwrap();
        assert!(from_alias.clear_feature_data);
    }

    #[test]
    fn expand_placeholders_replaces_accession_and_lineage_in_config_paths() {
        let mut cfg = ImportConfig {
            assembly: AssemblyImportConfig {
                accession: "GCA_00000001.1".to_string(),
                taxon_id: Some("1234".to_string()),
                ancestors: vec![],
                lineage: vec![],
            },
            es: EsConfig {
                host: "http://localhost".to_string(),
                port: 9200,
                username: None,
                password: None,
                hub: HubConfig {
                    name: "goat".to_string(),
                    release: "2021.10.15".to_string(),
                    taxonomy: "ncbi".to_string(),
                },
            },
            sequence_report: SequenceReportImportConfig {
                accession: "GCA_00000001.1".to_string(),
                taxon_id: "1234".to_string(),
                ancestors: vec![],
                path: None,
                local_path: Some(PathBuf::from("~/tmp/{ACCESSION}.sequence_report.jsonl")),
            },
            bed: MultiBedConfig {
                accession: "GCA_00000001.1".to_string(),
                taxon_id: "1234".to_string(),
                ancestors: vec![],
                lines_per_unit: 1000,
                window_specs: vec![],
                bed_configs: vec![crate::parse::bed::BedConfig {
                    path: PathBuf::from("https://example.org/{ACCESSION}/track.bed.gz"),
                    local_path: Some(PathBuf::from("~/tmp/{ACCESSION}.track.bed.gz")),
                    value_columns: vec![],
                    has_header: false,
                }],
            },
            busco: MultiBuscoConfig {
                accession: "GCA_00000001.1".to_string(),
                taxon_id: "1234".to_string(),
                ancestors: vec![],
                tables: Some(vec![crate::parse::busco::BuscoTableConfig {
                    path: PathBuf::from("https://example.org/{ACCESSION}/{LINEAGE}/{ACCESSION}.{LINEAGE}.full_table.tsv.gz"),
                    local_path: Some(PathBuf::from("~/tmp/{ACCESSION}.{LINEAGE}.full_table.tsv.gz")),
                    lineages: Some(vec!["diptera_odb12".to_string()]),
                }]),
                files: None,
                algs: None,
            },
            import: None,
        };

        expand_placeholders(&mut cfg);

        assert_eq!(
            cfg.sequence_report.local_path,
            Some(PathBuf::from("~/tmp/GCA_00000001.1.sequence_report.jsonl"))
        );
        assert_eq!(
            cfg.bed.bed_configs[0].local_path,
            Some(PathBuf::from("~/tmp/GCA_00000001.1.track.bed.gz"))
        );
        assert_eq!(
            cfg.busco.files.as_ref().unwrap()[0].path,
            PathBuf::from("https://example.org/GCA_00000001.1/diptera_odb12/GCA_00000001.1.diptera_odb12.full_table.tsv.gz")
        );
    }

    #[test]
    fn expand_placeholders_keeps_existing_busco_files_for_staged_configs() {
        let mut cfg = ImportConfig {
            assembly: AssemblyImportConfig {
                accession: "GCA_00000001.1".to_string(),
                taxon_id: Some("1234".to_string()),
                ancestors: vec![],
                lineage: vec![],
            },
            es: EsConfig {
                host: "http://localhost".to_string(),
                port: 9200,
                username: None,
                password: None,
                hub: HubConfig {
                    name: "goat".to_string(),
                    release: "2021.10.15".to_string(),
                    taxonomy: "ncbi".to_string(),
                },
            },
            sequence_report: SequenceReportImportConfig {
                accession: "GCA_00000001.1".to_string(),
                taxon_id: "1234".to_string(),
                ancestors: vec![],
                path: None,
                local_path: Some(PathBuf::from("~/tmp/{ACCESSION}.sequence_report.jsonl")),
            },
            bed: MultiBedConfig {
                accession: "GCA_00000001.1".to_string(),
                taxon_id: "1234".to_string(),
                ancestors: vec![],
                lines_per_unit: 1000,
                window_specs: vec![],
                bed_configs: vec![],
            },
            busco: MultiBuscoConfig {
                accession: "GCA_00000001.1".to_string(),
                taxon_id: "1234".to_string(),
                ancestors: vec![],
                tables: None,
                files: Some(vec![BuscoFileConfig {
                    path: PathBuf::from("https://example.org/{ACCESSION}/busco/{LINEAGE}/{ACCESSION}.{LINEAGE}.full_table.tsv.gz"),
                    local_path: Some(PathBuf::from("~/tmp/{ACCESSION}.{LINEAGE}.full_table.tsv.gz")),
                    lineage: "diptera_odb12".to_string(),
                    taxon_id: "1234".to_string(),
                    accession: "GCA_00000001.1".to_string(),
                    ancestors: vec![],
                }]),
                algs: None,
            },
            import: None,
        };

        expand_placeholders(&mut cfg);

        assert_eq!(
            cfg.busco.files.as_ref().unwrap().len(),
            1,
            "staged BUSCO sources must survive placeholder expansion"
        );
        assert_eq!(
            cfg.busco.files.as_ref().unwrap()[0].path,
            PathBuf::from(
                "https://example.org/GCA_00000001.1/busco/diptera_odb12/GCA_00000001.1.diptera_odb12.full_table.tsv.gz"
            )
        );
        assert_eq!(
            cfg.busco.files.as_ref().unwrap()[0].local_path,
            Some(PathBuf::from(
                "~/tmp/GCA_00000001.1.diptera_odb12.full_table.tsv.gz"
            ))
        );
    }

    #[test]
    fn resolve_taxon_batch_uses_family_then_order_then_class_then_phylum_fallback() {
        let lineage = vec![
            TaxonLineageEntry {
                taxon_id: "1000".to_string(),
                rank: "family".to_string(),
                scientific_name: Some("Testaceae".to_string()),
            },
            TaxonLineageEntry {
                taxon_id: "2000".to_string(),
                rank: "order".to_string(),
                scientific_name: Some("Testales".to_string()),
            },
        ];
        let batch_members = HashMap::from([
            (
                "family:1000".to_string(),
                vec![
                    "1".to_string(),
                    "2".to_string(),
                    "3".to_string(),
                    "4".to_string(),
                    "5".to_string(),
                ],
            ),
            (
                "order:2000".to_string(),
                vec!["6".to_string(), "7".to_string(), "8".to_string()],
            ),
        ]);

        let resolution =
            resolve_taxon_batch_for_metric(&lineage, &batch_members, MetricFamily::Gc, 5).unwrap();

        assert_eq!(resolution.level, BatchLevel::Family);
        assert_eq!(resolution.members.len(), 5);
        assert_eq!(resolution.batch_id, "family:1000:gc");
    }

    #[test]
    fn resolve_taxon_batch_rejects_undersized_batches() {
        let lineage = vec![TaxonLineageEntry {
            taxon_id: "4000".to_string(),
            rank: "phylum".to_string(),
            scientific_name: Some("Testophyta".to_string()),
        }];
        let batch_members = HashMap::from([(
            "phylum:4000".to_string(),
            vec![
                "1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "4".to_string(),
            ],
        )]);

        let err =
            resolve_taxon_batch_for_metric(&lineage, &batch_members, MetricFamily::Coverage, 5)
                .unwrap_err();

        assert!(err.to_string().contains("no stable batch"));
    }

    #[test]
    fn normalization_cache_keeps_biological_and_technical_metric_entries_separate() {
        let mut cache = NormalizationBaselineCache::new();
        let summary_gc = NormalizationSummary {
            batch_size: 5,
            mean: 0.5,
            std_dev: 0.1,
            median: Some(0.5),
            mad: Some(0.1),
            version: "phase-2b".to_string(),
        };
        let summary_coverage = NormalizationSummary {
            batch_size: 5,
            mean: 3.0,
            std_dev: 0.2,
            median: Some(3.0),
            mad: Some(0.2),
            version: "phase-2b".to_string(),
        };

        cache.insert(
            "family:1000",
            &MetricFamily::Gc,
            &CoordinateSystem::PhysicalWindow,
            "phase-2b",
            summary_gc.clone(),
        );
        cache.insert(
            "family:1000",
            &MetricFamily::Coverage,
            &CoordinateSystem::PhysicalWindow,
            "phase-2b",
            summary_coverage.clone(),
        );

        assert_eq!(
            cache.get(
                "family:1000",
                &MetricFamily::Gc,
                &CoordinateSystem::PhysicalWindow,
                "phase-2b"
            ),
            Some(&summary_gc)
        );
        assert_eq!(
            cache.get(
                "family:1000",
                &MetricFamily::Coverage,
                &CoordinateSystem::PhysicalWindow,
                "phase-2b"
            ),
            Some(&summary_coverage)
        );
    }

    #[test]
    fn batch_manifest_requires_min_members_and_valid_members() {
        let manifest = BatchManifest {
            schema_version: 1,
            taxonomy: "ncbi".to_string(),
            generated_at: Some("2026-09-04T00:00:00Z".to_string()),
            min_members: 5,
            defaults: BatchDefaults { template: None },
            batches: vec![BatchConfig {
                id: "family:1234".to_string(),
                rank: "family".to_string(),
                taxon_id: Some("1234".to_string()),
                taxon_name: Some("Testaceae".to_string()),
                template: None,
                members: vec![
                    BatchMember {
                        accession: "GCA_00000001.1".to_string(),
                        taxon_id: Some("1234".to_string()),
                    },
                    BatchMember {
                        accession: "GCA_00000002.1".to_string(),
                        taxon_id: Some("1234".to_string()),
                    },
                    BatchMember {
                        accession: "GCA_00000003.1".to_string(),
                        taxon_id: Some("1234".to_string()),
                    },
                    BatchMember {
                        accession: "GCA_00000004.1".to_string(),
                        taxon_id: Some("1234".to_string()),
                    },
                ],
            }],
        };

        let err = validate_batch_manifest(&manifest).unwrap_err();
        assert!(err.to_string().contains("below the minimum"));
    }

    #[test]
    fn batch_manifest_accepts_valid_template_and_members() {
        let manifest = BatchManifest {
            schema_version: 1,
            taxonomy: "ncbi".to_string(),
            generated_at: Some("2026-09-04T00:00:00Z".to_string()),
            min_members: 5,
            defaults: BatchDefaults {
                template: Some(BatchTemplateRef {
                    path: std::path::PathBuf::from("./templates/import.config.yaml"),
                }),
            },
            batches: vec![BatchConfig {
                id: "family:1234".to_string(),
                rank: "family".to_string(),
                taxon_id: Some("1234".to_string()),
                taxon_name: Some("Testaceae".to_string()),
                template: Some(BatchTemplateRef {
                    path: std::path::PathBuf::from("./templates/family_import.config.yaml"),
                }),
                members: vec![
                    BatchMember {
                        accession: "GCA_00000001.1".to_string(),
                        taxon_id: Some("1234".to_string()),
                    },
                    BatchMember {
                        accession: "GCA_00000002.1".to_string(),
                        taxon_id: Some("1234".to_string()),
                    },
                    BatchMember {
                        accession: "GCA_00000003.1".to_string(),
                        taxon_id: Some("1234".to_string()),
                    },
                    BatchMember {
                        accession: "GCA_00000004.1".to_string(),
                        taxon_id: Some("1234".to_string()),
                    },
                    BatchMember {
                        accession: "GCA_00000005.1".to_string(),
                        taxon_id: Some("1234".to_string()),
                    },
                ],
            }],
        };

        validate_batch_manifest(&manifest).unwrap();
    }

    #[test]
    fn batch_manifest_accepts_batch_member_list() {
        let manifest = BatchManifest {
            schema_version: 1,
            taxonomy: "ncbi".to_string(),
            generated_at: Some("2026-09-04T00:00:00Z".to_string()),
            min_members: 2,
            defaults: BatchDefaults { template: None },
            batches: vec![BatchConfig {
                id: "family:5678".to_string(),
                rank: "family".to_string(),
                taxon_id: Some("5678".to_string()),
                taxon_name: Some("Testaceae".to_string()),
                template: None,
                members: vec![
                    BatchMember {
                        accession: "GCA_00000006.1".to_string(),
                        taxon_id: None,
                    },
                    BatchMember {
                        accession: "GCA_00000007.1".to_string(),
                        taxon_id: None,
                    },
                    BatchMember {
                        accession: "GCA_00000008.1".to_string(),
                        taxon_id: None,
                    },
                ],
            }],
        };

        validate_batch_manifest(&manifest).unwrap();
    }

    #[test]
    fn batch_manifest_rejects_empty_member_list() {
        let manifest = BatchManifest {
            schema_version: 1,
            taxonomy: "ncbi".to_string(),
            generated_at: Some("2026-09-04T00:00:00Z".to_string()),
            min_members: 1,
            defaults: BatchDefaults { template: None },
            batches: vec![BatchConfig {
                id: "family:9999".to_string(),
                rank: "family".to_string(),
                taxon_id: Some("9999".to_string()),
                taxon_name: Some("Testaceae".to_string()),
                template: None,
                members: vec![],
            }],
        };

        let err = validate_batch_manifest(&manifest).unwrap_err();
        assert!(err.to_string().contains("no members"));
    }

    #[test]
    fn batch_manifest_rejects_missing_member_subset() {
        let manifest = BatchManifest {
            schema_version: 1,
            taxonomy: "ncbi".to_string(),
            generated_at: Some("2026-09-04T00:00:00Z".to_string()),
            min_members: 1,
            defaults: BatchDefaults { template: None },
            batches: vec![BatchConfig {
                id: "family:1000".to_string(),
                rank: "family".to_string(),
                taxon_id: Some("1000".to_string()),
                taxon_name: Some("Testaceae".to_string()),
                template: None,
                members: vec![],
            }],
        };

        let err = validate_batch_manifest(&manifest).unwrap_err();
        assert!(err.to_string().contains("no members"));
    }

    #[test]
    fn batch_manifest_expands_relative_templates_and_uses_batch_taxon_fallback() {
        let temp_dir = std::env::temp_dir().join(format!(
            "blobtk-batch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("templates")).unwrap();
        std::fs::write(
            temp_dir.join("templates/import.config.yaml"),
            r#"
es:
  host: "http://localhost"
  port: 9200
  hub:
    name: goat
    release: 2021.10.15
    taxonomy: ncbi

assembly:
  accession: "{ACCESSION}"
  taxon_id: "{TAXON_ID}"

sequence_report:
  accession: "{ACCESSION}"
  taxon_id: "{TAXON_ID}"
  local_path: "{LOCAL_ROOT}/family/sequence_reports/{ACCESSION}.jsonl"

bed:
  accession: "{ACCESSION}"
  taxon_id: "{TAXON_ID}"
  lines_per_unit: 1000
  windows:
    - type: size
      size: 1000000
      remnant_policy: Centered
  files:
    - path: "{REMOTE_ROOT}/family/beds/{ACCESSION}.GC.1k.bedGraph.gz"
      local_path: "{LOCAL_ROOT}/family/beds/{ACCESSION}.GC.1k.bedGraph.gz"
      value_columns:
        - label: gc
          index: 3
          type: float
          summary_functions:
            - name: mean

busco:
  accession: "{ACCESSION}"
  taxon_id: "{TAXON_ID}"
  tables: []

import:
  entity_types:
    - sequence
    - window
    - busco
    - attribute
"#,
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("batch.config.yaml"),
            r#"
schema_version: 1
taxonomy: ncbi
min_members: 1

defaults:
  template:
    path: ./templates/import.config.yaml

batches:
  - id: family:1234
    rank: family
    taxon_id: "1234"
    taxon_name: "Testaceae"
    members:
      - accession: GCA_00000001.1
"#,
        )
        .unwrap();

        let configs = expand_batch_member_configs_for_manifest(
            &temp_dir.join("batch.config.yaml"),
            "family:1234",
            "https://example.org/data",
            "/tmp/blobtk",
        )
        .unwrap();

        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].assembly.accession, "GCA_00000001.1");
        assert_eq!(configs[0].assembly.taxon_id, Some("1234".to_string()));
        assert_eq!(
            configs[0].sequence_report.local_path,
            Some(std::path::PathBuf::from(
                "/tmp/blobtk/family/sequence_reports/GCA_00000001.1.jsonl"
            ))
        );
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn resolve_assembly_taxon_id_uses_lookup_when_taxon_missing() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server_url = format!("http://{addr}");

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0; 2048];
            let _ = stream.read(&mut buf).unwrap();
            let response = br#"{
                "status":{"success":true},
                "records":[{"record":{"taxon_id":"1518534"}}]
            }"#;
            let http = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                String::from_utf8_lossy(response)
            );
            stream.write_all(http.as_bytes()).unwrap();
        });

        let resolved =
            lookup_taxon_id_for_assembly("GCA_016920705.1", "ncbi", &server_url).unwrap();
        assert_eq!(resolved, "1518534");
        handle.join().unwrap();
    }

    #[test]
    fn resolve_assembly_taxon_id_uses_process_cache_for_repeated_calls() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server_url = format!("http://{addr}");

        let request_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let request_count_for_server = request_count.clone();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0; 2048];
            let _ = stream.read(&mut buf).unwrap();
            request_count_for_server.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let response = br#"{
                "status":{"success":true},
                "records":[{"record":{"taxon_id":"1518534"}}]
            }"#;
            let http = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                String::from_utf8_lossy(response)
            );
            stream.write_all(http.as_bytes()).unwrap();
        });

        let cached_key = "GCA_CACHE_TEST_1.1";
        let first = lookup_taxon_id_for_assembly(cached_key, "ncbi", &server_url).unwrap();
        let second = lookup_taxon_id_for_assembly(cached_key, "ncbi", &server_url).unwrap();
        assert_eq!(first, "1518534");
        assert_eq!(second, "1518534");
        assert_eq!(request_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        handle.join().unwrap();
    }

    #[test]
    fn import_config_allows_missing_taxon_id() {
        let yaml = r#"
es:
  host: "http://localhost"
  port: 9200
  hub:
    name: goat
    release: 2021.10.15
    taxonomy: ncbi
assembly:
  accession: GCA_016920705.1
sequence_report:
  accession: GCA_016920705.1
  local_path: "~/tmp/GCA_016920705.1.sequence_report.jsonl"
bed:
  accession: GCA_016920705.1
  lines_per_unit: 1000
  windows:
    - type: size
      size: 1000000
      remnant_policy: Centered
  files: []
busco:
  accession: GCA_016920705.1
  algs: []
"#;

        let cfg: ImportConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.assembly.taxon_id, None);
        assert_eq!(cfg.sequence_report.taxon_id, "");
        assert_eq!(cfg.bed.taxon_id, "");
        assert_eq!(cfg.busco.taxon_id, "");
    }

    #[test]
    fn validate_bed_sequence_coverage_rejects_sequence_id_mismatch() {
        let seq_features = HashMap::from([(
            "CM000001.1".to_string(),
            FeatureDocument::new(
                "CM000001.1".to_string(),
                None,
                "chromosome".to_string(),
                1,
                1_000_000,
                Some(1),
                None,
                "CM000001.1".to_string(),
                1_000_000,
                "asm-1".to_string(),
                "tax-1".to_string(),
                None,
                None,
                None,
            ),
        )]);

        let bed_cfg = MultiBedConfig {
            accession: "GCA_test".to_string(),
            taxon_id: "123".to_string(),
            ancestors: vec![],
            lines_per_unit: 1000,
            bed_configs: vec![crate::parse::bed::BedConfig {
                path: std::path::PathBuf::from("/tmp/mismatch.bed"),
                local_path: Some(std::path::PathBuf::from("/tmp/mismatch.bed")),
                value_columns: vec![crate::parse::bed::ValueColumn {
                    label: "gc".to_string(),
                    index: 3,
                    value_type: "float".to_string(),
                    summary_functions: vec![crate::parse::bed::SummaryFunction::Mean],
                    normalisation: None,
                }],
                has_header: false,
            }],
            window_specs: vec![],
        };

        std::fs::write("/tmp/mismatch.bed", "chr1\t0\t1000\t0.2\n").unwrap();

        let err = validate_bed_sequence_coverage(&seq_features, &bed_cfg).unwrap_err();
        assert!(err
            .to_string()
            .contains("BED and sequence report IDs do not match"));
    }

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
    fn create_attribute_docs_from_features_accepts_sequence_and_window_feature_type_aliases() {
        let mut state = ImportState::new("asm-1".to_string(), "tax-1".to_string());
        let es_cfg = EsConfig {
            host: "http://localhost:9200".to_string(),
            port: 9200,
            username: None,
            password: None,
            hub: HubConfig {
                name: "test".to_string(),
                release: "test".to_string(),
                taxonomy: "test".to_string(),
            },
        };
        let import_opts = Some(ImportOptions {
            clear_feature_data: false,
            entity_types: Some(vec![]),
            busco_tallies: None,
            synteny_index: None,
        });

        let sequence_doc = FeatureDocument::new(
            "CM000001.1".to_string(),
            None,
            "chromosome".to_string(),
            1,
            1_000_000,
            Some(1),
            None,
            "CM000001.1".to_string(),
            1_000_000,
            "asm-1".to_string(),
            "tax-1".to_string(),
            None,
            None,
            None,
        );
        let window_doc = FeatureDocument::new(
            "CM000001.1:0-2000:win-2k".to_string(),
            Some("CM000001.1".to_string()),
            "win-2k".to_string(),
            0,
            2000,
            None,
            None,
            "CM000001.1".to_string(),
            1_000_000,
            "asm-1".to_string(),
            "tax-1".to_string(),
            None,
            None,
            None,
        );

        create_attribute_docs_from_features(
            &[sequence_doc, window_doc],
            &mut state,
            &es_cfg,
            &import_opts,
        )
        .unwrap();

        assert!(!state.attribute_doc_cache.documents.is_empty());
    }

    #[test]
    fn sequence_and_window_import_contract_stays_single_index_via_config_flow() {
        use crate::parse::bed::{
            parse_bed_files, BedConfig, MultiBedConfig, RemnantPolicy, SummaryFunction,
            ValueColumn, WindowSpec,
        };
        use crate::parse::sequence_report::parse_sequence_report;

        let tmp_dir = std::env::temp_dir().join("blobtk_phase1_single_index_regression");
        let _ = std::fs::create_dir_all(&tmp_dir);

        let seq_report_path = tmp_dir.join("sequence_report.jsonl");
        let bed_path = tmp_dir.join("values.bed");

        std::fs::write(
            &seq_report_path,
            "{\"assembly_accession\":\"GCA_test\",\"assigned_molecule_location_type\":\"chromosome\",\"chr_name\":\"chr1\",\"gc_percent\":42.5,\"genbank_accession\":\"chr1\",\"length\":5000,\"role\":\"assembled-molecule\",\"sequence_name\":\"chr1\"}\n",
        )
        .unwrap();

        std::fs::write(
            &bed_path,
            "chr1\t0\t1000\t0.10\nchr1\t1000\t2000\t0.20\nchr1\t2000\t3000\t0.30\nchr1\t3000\t4000\t0.40\nchr1\t4000\t5000\t0.50\n",
        )
        .unwrap();

        let seq_features = parse_sequence_report(SequenceReportImportConfig {
            accession: "GCA_test".to_string(),
            taxon_id: "123".to_string(),
            ancestors: vec!["1".to_string(), "2".to_string()],
            path: None,
            local_path: Some(seq_report_path.clone()),
        })
        .unwrap();

        let bed_config = MultiBedConfig {
            accession: "GCA_test".to_string(),
            taxon_id: "123".to_string(),
            ancestors: vec!["1".to_string(), "2".to_string()],
            lines_per_unit: 1000,
            bed_configs: vec![BedConfig {
                path: bed_path.clone(),
                local_path: Some(bed_path.clone()),
                value_columns: vec![ValueColumn {
                    label: "gc".to_string(),
                    index: 3,
                    value_type: "float".to_string(),
                    summary_functions: vec![SummaryFunction::Mean],
                    normalisation: None,
                }],
                has_header: false,
            }],
            window_specs: vec![WindowSpec::Size {
                size: 2000,
                remnant_policy: RemnantPolicy::Centered,
                remnant_bounds: None,
            }],
        };

        let window_docs = parse_bed_files(&bed_config, None).unwrap();
        let sequence_doc = seq_features.values().next().unwrap().clone();
        let window_doc = window_docs.values().next().unwrap().clone();

        let mut state = ImportState::new("GCA_test".to_string(), "123".to_string());
        let es_cfg = EsConfig {
            host: "http://localhost:9200".to_string(),
            port: 9200,
            username: None,
            password: None,
            hub: HubConfig {
                name: "test".to_string(),
                release: "test".to_string(),
                taxonomy: "test".to_string(),
            },
        };
        let import_opts = Some(ImportOptions {
            clear_feature_data: false,
            entity_types: Some(vec!["sequence".to_string(), "window".to_string()]),
            busco_tallies: None,
            synteny_index: None,
        });

        create_attribute_docs_from_features(
            &[sequence_doc, window_doc],
            &mut state,
            &es_cfg,
            &import_opts,
        )
        .unwrap();

        assert!(!state.attribute_doc_cache.documents.is_empty());
        assert!(state
            .attribute_doc_cache
            .documents
            .values()
            .any(|doc| doc.name == "feature_type"));
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
