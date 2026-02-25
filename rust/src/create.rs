use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufRead;
use std::path::PathBuf;
use std::vec;

use anyhow::Result;
use serde::Serialize;
use serde_json;

use crate::blobdir::{AssemblyMeta, Datatype, Field, FieldMeta, Meta, PlotMeta, TaxonMeta};
use crate::io;

fn parse_fasta(fasta: &PathBuf) -> Result<Vec<(String, usize, f64, usize)>> {
    // returns (id, length, gc_fraction, ncount)
    let mut seqs: Vec<(String, usize, f64, usize)> = Vec::new();
    let reader = io::file_reader(fasta.clone())?;
    let mut current_id: Option<String> = None;
    let mut current_len: usize = 0;
    let mut current_atgc: usize = 0;
    let mut current_gc: usize = 0;
    for line_res in reader.lines() {
        let line = line_res?;
        if line.starts_with('>') {
            if let Some(id) = current_id.take() {
                let non_atgc_count = current_len.saturating_sub(current_atgc);
                let gc_frac = if current_atgc == 0 {
                    0.0
                } else {
                    (current_gc as f64) / (current_atgc as f64)
                };
                seqs.push((id, current_len, gc_frac, non_atgc_count));
            }
            // new id
            let header = line.trim_start_matches('>').trim().to_string();
            let id = header
                .split_whitespace()
                .next()
                .unwrap_or(&header)
                .to_string();
            current_id = Some(id);
            current_len = 0;
            current_atgc = 0;
            current_gc = 0;
        } else {
            let seq_line = line.trim().as_bytes();
            for &b in seq_line.iter() {
                current_len += 1;
                match b {
                    b'G' | b'g' => {
                        current_gc += 1;
                        current_atgc += 1;
                    }
                    b'C' | b'c' => {
                        current_gc += 1;
                        current_atgc += 1;
                    }
                    b'A' | b'a' | b'T' | b't' | b'U' | b'u' => {
                        current_atgc += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    // process last record
    if let Some(id) = current_id {
        let non_atgc_count = current_len.saturating_sub(current_atgc);
        let gc_frac = if current_atgc == 0 {
            0.0
        } else {
            (current_gc as f64) / (current_atgc as f64)
        };
        seqs.push((id, current_len, gc_frac, non_atgc_count));
    }
    Ok(seqs)
}

#[derive(Serialize, Debug, Default, Clone)]
struct BuscoMeta {
    version: Option<String>,
    lineage: Option<String>,
    n_buscos: Option<usize>,
    headers: Option<Vec<String>>,
}

fn parse_busco(busco: &PathBuf) -> Result<(HashMap<String, Vec<(String, String)>>, BuscoMeta)> {
    // Tolerant parser for BUSCO full_table.gz-like files.
    // Extracts mapping seq -> Vec<(gene, status)> and top comment metadata.
    let reader = io::file_reader(busco.clone())?;
    let mut map: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut meta = BuscoMeta::default();
    let mut header_cols: Option<Vec<String>> = None;
    let mut gene_idx: Option<usize> = None;
    let mut seq_idx: Option<usize> = None;
    let mut status_idx: Option<usize> = None;

    for line_res in reader.lines() {
        let line = line_res?;
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with('#') {
            let content = line.trim_start_matches('#').trim();
            let low = content.to_lowercase();
            // Try to extract metadata
            if low.contains("version") && meta.version.is_none() {
                if let Some(cap) = content.split(":\\s*").nth(1) {
                    meta.version = Some(cap.split_whitespace().next().unwrap_or(cap).to_string());
                }
            }
            if (low.contains("lineage") || low.contains("dataset")) && meta.lineage.is_none() {
                if let Some(pos) = content.find("is:") {
                    let after_is = &content[pos + 3..].trim();
                    if let Some(end) = after_is.find(|c: char| c == '(' || c == ',') {
                        meta.lineage = Some(after_is[..end].trim().to_string());
                    } else {
                        meta.lineage = Some(after_is.to_string());
                    }
                }
            }
            // Detect header row e.g. "# Busco id\tSequence\tStatus"
            if header_cols.is_none()
                && (low.contains("busco") || low.contains("sequence") || low.contains("status"))
                && content.contains('\t')
            {
                let cols: Vec<String> = content.split('\t').map(|s| s.trim().to_string()).collect();
                // record headers
                meta.headers = Some(cols.clone());
                header_cols = Some(cols.clone());
                for (i, h) in cols.iter().enumerate() {
                    let lh = h.to_lowercase();
                    if gene_idx.is_none() && (lh.contains("busco") || lh.contains("gene")) {
                        gene_idx = Some(i);
                    }
                    if seq_idx.is_none() && (lh.contains("sequence") || lh.contains("seq")) {
                        seq_idx = Some(i);
                    }
                    if status_idx.is_none() && (lh.contains("status") || lh.contains("status")) {
                        status_idx = Some(i);
                    }
                }
            }
            continue;
        }

        // Non-comment data line
        let cols: Vec<&str> = line.split('\t').collect();
        if header_cols.is_some()
            && (gene_idx.is_some() || seq_idx.is_some() || status_idx.is_some())
        {
            let gi = gene_idx.unwrap_or(0);
            let si = seq_idx.unwrap_or(1.min(cols.len().saturating_sub(1)));
            let sti = status_idx.unwrap_or(2.min(cols.len().saturating_sub(1)));
            let gene = cols
                .get(gi)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let seq = cols
                .get(si)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let status = cols
                .get(sti)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            map.entry(seq).or_default().push((gene, status));
            let mut unique_buscos: HashSet<String> = HashSet::new();
            for (_, entries) in map.iter() {
                for (gene, _status) in entries.iter() {
                    if gene != "unknown" {
                        unique_buscos.insert(gene.clone());
                    }
                }
            }
            meta.n_buscos = Some(unique_buscos.len());
        } else {
            // warn unsupported format
            eprintln!(
                "Warning: BUSCO file {} has unrecognized format",
                busco.to_string_lossy()
            );
            break;
        }
    }
    Ok((map, meta))
}

pub fn create(options: &crate::cli::CreateOptions) -> Result<(), anyhow::Error> {
    let fasta_path = match &options.fasta {
        Some(p) => p.clone(),
        None => return Err(anyhow::anyhow!("--fasta is required for create")),
    };

    let file_name = fasta_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("assembly");

    let prefix = if file_name.to_lowercase().ends_with(".gz") {
        &file_name[..file_name.len() - 3]
    } else {
        file_name
    };

    let out_dir = options.out.clone().unwrap_or_else(|| {
        let base = std::path::Path::new(prefix)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("assembly");
        PathBuf::from(format!("{}_blobdir", base))
    });
    std::fs::create_dir_all(&out_dir)?;

    let seqs = parse_fasta(&fasta_path)?;
    let seq_count = seqs.len();
    // Write identifiers.json
    let ids: Vec<String> = seqs.iter().map(|(id, _, _, _)| id.clone()).collect();
    let identifiers_field = Field {
        values: ids.clone(),
        keys: vec![],
        category_slot: None,
        headers: None,
    };
    let id_path = out_dir.join("identifiers.json");
    let id_file = File::create(id_path)?;
    serde_json::to_writer_pretty(id_file, &identifiers_field)?;

    // Write length.json
    let lengths: Vec<usize> = seqs.iter().map(|(_, l, _, _)| *l).collect();
    let length_field = Field {
        values: lengths.clone(),
        keys: vec![],
        category_slot: None,
        headers: None,
    };
    let len_path = out_dir.join("length.json");
    let len_file = File::create(len_path)?;
    serde_json::to_writer_pretty(len_file, &length_field)?;

    // Write gc.json (float field)
    let gcs: Vec<f64> = seqs.iter().map(|(_, _, gc, _)| *gc).collect();
    let gc_field = Field {
        values: gcs.clone(),
        keys: vec![],
        category_slot: None,
        headers: None,
    };
    let gc_path = out_dir.join("gc.json");
    let gc_file = File::create(gc_path)?;
    serde_json::to_writer_pretty(gc_file, &gc_field)?;

    // Write n.json (proportion of non-ATGC bases)
    let n_props: Vec<f64> = seqs
        .iter()
        .map(|(_, l, _, ncount)| {
            if *l == 0 {
                0.0
            } else {
                (*ncount as f64) / (*l as f64)
            }
        })
        .collect();
    let n_field = Field {
        values: n_props.clone(),
        keys: vec![],
        category_slot: None,
        headers: None,
    };
    let n_path = out_dir.join("n.json");
    let n_file = File::create(n_path)?;
    serde_json::to_writer_pretty(n_file, &n_field)?;

    // Write ncount.json (integer counts of non-ATGC bases)
    let ncounts: Vec<usize> = seqs.iter().map(|(_, _, _, ncount)| *ncount).collect();
    let ncount_field = Field {
        values: ncounts.clone(),
        keys: vec![],
        category_slot: None,
        headers: None,
    };
    let ncount_path = out_dir.join("ncount.json");
    let ncount_file = File::create(ncount_path)?;
    serde_json::to_writer_pretty(ncount_file, &ncount_field)?;

    // If BUSCO provided, parse it now so we can include metadata in meta.json
    let mut maybe_busco_map: Option<HashMap<String, Vec<(String, String)>>> = None;
    let mut maybe_busco_meta: Option<BuscoMeta> = None;
    if let Some(busco_path) = &options.busco {
        let (bm, bmeta) = parse_busco(busco_path)?;
        maybe_busco_map = Some(bm);
        maybe_busco_meta = Some(bmeta);
    }
    // If BUSCO provided, write a busco.json file using previously parsed data
    if let Some(_) = &options.busco {
        let busco_map = maybe_busco_map
            .as_ref()
            .expect("busco map should be parsed earlier");
        let busco_meta = maybe_busco_meta.as_ref().cloned().unwrap_or_default();
        // Collect unique statuses
        let mut statuses: Vec<String> = Vec::new();
        for (_, entries) in busco_map.iter() {
            for (_gene, status) in entries.iter() {
                if !statuses.contains(status) {
                    statuses.push(status.clone())
                }
            }
        }
        if statuses.is_empty() {
            statuses.push("unknown".to_string());
        }
        // Build values: for each sequence in order, a Vec<(String, usize)> tuples
        let mut values: Vec<Vec<(String, usize)>> = Vec::new();
        let status_index: HashMap<String, usize> = statuses
            .iter()
            .enumerate()
            .map(|(i, s)| (s.clone(), i))
            .collect();
        for (id, _, _, _) in seqs.iter() {
            let mut seq_vals: Vec<(String, usize)> = Vec::new();
            if let Some(entries) = busco_map.get(id) {
                for (gene, status) in entries.iter() {
                    let idx = *status_index.get(status).unwrap_or(&0usize);
                    seq_vals.push((gene.clone(), idx));
                }
            }
            values.push(seq_vals);
        }

        let busco_field = Field {
            values,
            keys: statuses.clone(),
            category_slot: Some(1),
            headers: Some(vec!["Busco id".to_string(), "Status".to_string()]),
        };
        let busco_filename = match &busco_meta.lineage {
            Some(lineage) => format!("{}_busco.json", lineage.to_lowercase(),),
            None => "busco.json".to_string(),
        };
        let busco_out = out_dir.join(busco_filename);
        let busco_file = File::create(busco_out)?;
        serde_json::to_writer_pretty(busco_file, &busco_field)?;
    }
    let span = seqs.iter().map(|(_, l, _, _)| *l).sum::<usize>();

    // Build meta
    let mut meta = Meta {
        id: String::from("minimal"),
        name: fasta_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "assembly".to_string()),
        record_type: String::from("scaffold"),
        records: seq_count,
        revision: 0,
        version: 1,
        assembly: AssemblyMeta {
            accession: "draft".to_string(),
            level: "scaffold".to_string(),
            prefix: Some(prefix.to_string()),
            file: Some(fasta_path),
            scaffold_count: Some(seq_count),
            span: Some(span),
            ..Default::default()
        },
        fields: vec![
            FieldMeta {
                id: "identifiers".to_string(),
                field_type: Some("identifiers".to_string()),
                datatype: Some(Datatype::String),
                count: Some(seq_count),
                preload: Some(true),
                active: Some(true),
                ..Default::default()
            },
            FieldMeta {
                id: "length".to_string(),
                field_type: Some("variable".to_string()),
                scale: Some("scaleLog".to_string()),
                datatype: Some(Datatype::Integer),
                range: Some([
                    *lengths.iter().min().unwrap_or(&0) as f64,
                    *lengths.iter().max().unwrap_or(&0) as f64,
                ]),
                ..Default::default()
            },
            FieldMeta {
                id: "gc".to_string(),
                field_type: Some("variable".to_string()),
                scale: Some("scaleLinear".to_string()),
                datatype: Some(Datatype::Float),
                range: Some([
                    gcs.iter().cloned().fold(f64::INFINITY, f64::min),
                    gcs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                ]),
                ..Default::default()
            },
            FieldMeta {
                id: "n".to_string(),
                field_type: Some("variable".to_string()),
                scale: Some("scaleLinear".to_string()),
                datatype: Some(Datatype::Float),
                range: Some([
                    n_props.iter().cloned().fold(f64::INFINITY, f64::min),
                    n_props.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                ]),
                ..Default::default()
            },
            FieldMeta {
                id: "ncount".to_string(),
                field_type: Some("variable".to_string()),
                scale: Some("scaleLinear".to_string()),
                datatype: Some(Datatype::Integer),
                range: Some([
                    ncounts.iter().cloned().min().unwrap_or(0) as f64,
                    ncounts.iter().cloned().max().unwrap_or(0) as f64,
                ]),
                ..Default::default()
            },
        ],
        plot: PlotMeta::default(),
        taxon: TaxonMeta {
            name: "unnamed".to_string(),
            taxid: "0".to_string(),
            ..Default::default()
        },
        ..Default::default()
    };

    // Write meta.json
    let meta_path = out_dir.join("meta.json");
    let meta_file = File::create(meta_path)?;
    // If we need to include the busco field, mutate meta.fields now
    if let Some(bmeta) = &maybe_busco_meta {
        if let Some(busco_path) = &options.busco {
            let set_name = bmeta
                .lineage
                .clone()
                .unwrap_or_else(|| "busco_set".to_string());
            let id = format!("{}_busco", set_name);
            let fm = FieldMeta {
                id: id,
                field_type: Some("multiarray".to_string()),
                datatype: Some(Datatype::Mixed),
                parent: Some("busco".to_string()),
                count: bmeta.n_buscos,
                odb_set: bmeta.lineage.clone(),
                file: Some(busco_path.to_string_lossy().to_string()),
                version: bmeta.version.clone(),
                category_slot: Some(1),
                headers: bmeta.headers.clone(),
                ..Default::default()
            };
            let parent_fm = FieldMeta {
                id: "busco".to_string(),
                field_type: Some("array".to_string()),
                datatype: Some(Datatype::Mixed),
                children: Some(vec![fm]),
                ..Default::default()
            };
            meta.fields.push(parent_fm);
        }
    }
    serde_json::to_writer_pretty(meta_file, &meta)?;

    Ok(())
}
