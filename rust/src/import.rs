//!
//! Invoked by calling:
//! `blobtk import <args>`

// use crate::index::es::config;
use serde::{Deserialize, Serialize};

use crate::parse::bed_data::{parse_bed_files, MultiBedConfig};
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
    // pub busco: MultiBuscoConfig,
}

fn expand_placeholders(cfg: &mut ImportConfig) {
    let accession = cfg.assembly.accession.clone();
    for bed in cfg.bed.bed_configs.iter_mut() {
        let s = bed.path.to_string_lossy().to_string();
        let s = s.replace("{ACCESSION}", &accession);
        bed.path = std::path::PathBuf::from(s);
    }
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
    let mut sequence_report_cfg = cfg.sequence_report;
    let sequence_features = sequence_report::parse_sequence_report(sequence_report_cfg)?;
    let json_sequence_features = serde_json::to_string_pretty(&sequence_features).unwrap();
    // print the json_sequence_features to stdout for inspection
    // println!("{}", &json_sequence_features);
    return Ok(());
    let mut bed_cfg = cfg.bed;
    let features = parse_bed_files(&bed_cfg).unwrap();
    let json_features = serde_json::to_string_pretty(&features).unwrap();
    // print the json_features to stdout for inspection
    // println!("{}", &json_features);

    Ok(())
}
