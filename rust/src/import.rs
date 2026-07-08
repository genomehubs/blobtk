//!
//! Invoked by calling:
//! `blobtk import <args>`

// use crate::index::es::config;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::parse::bed::{parse_bed_files, MultiBedConfig};
use crate::parse::busco::{BuscoFileConfig, MultiBuscoConfig};
use crate::parse::sequence_report;

#[derive(Deserialize, Serialize, Debug)]
pub struct EsConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct HubImportConfig {
    pub name: String,
    pub release: String,
    pub taxonomy: String,
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

#[derive(Deserialize, Serialize, Debug)]
pub struct ImportConfig {
    pub assembly: AssemblyImportConfig,
    pub es: EsConfig,
    pub hub: HubImportConfig,
    pub sequence_report: SequenceReportImportConfig,
    pub bed: MultiBedConfig,
    pub busco: MultiBuscoConfig,
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

pub fn import(options: &crate::cli::ImportOptions) -> Result<(), anyhow::Error> {
    let config_path = &options.config;
    dbg!(&config_path);
    let yaml_text = std::fs::read_to_string(config_path)?;
    let mut cfg: ImportConfig = serde_yaml::from_str(&yaml_text)?;
    expand_placeholders(&mut cfg);
    print!("Parsed config: {:#?}", cfg);
    // return Ok(());
    let sequence_report_cfg = cfg.sequence_report;
    let sequence_features = sequence_report::parse_sequence_report(sequence_report_cfg)?;
    let json_sequence_features = serde_json::to_string_pretty(&sequence_features).unwrap();
    // print the json_sequence_features to stdout for inspection
    println!("{}", &json_sequence_features);
    return Ok(());
    let window_cfg = cfg.bed.window_specs.clone();
    let busco_cfg = cfg.busco;
    let busco_features =
        crate::parse::busco::parse_busco_files(&busco_cfg, &sequence_features, window_cfg).unwrap();
    let json_busco_features = serde_json::to_string_pretty(&busco_features).unwrap();
    // print the json_busco_features to stdout for inspection
    println!("{}", &json_busco_features);

    let bed_cfg = cfg.bed;
    let features = parse_bed_files(&bed_cfg).unwrap();
    let json_features = serde_json::to_string_pretty(&features).unwrap();
    // print the json_features to stdout for inspection
    // println!("{}", &json_features);

    Ok(())
}
