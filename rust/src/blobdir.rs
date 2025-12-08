use std::cmp::{max, min};
use std::collections::{HashMap, HashSet};
use std::f64::{INFINITY, NEG_INFINITY};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use flate2::read::GzDecoder;
use glob::glob;
use reqwest;
use serde;
use serde::{Deserialize, Serialize};
use serde_aux::prelude::*;
use serde_json;
use serde_with::{serde_as, DefaultOnError};
use titlecase::titlecase;
use url::Url;

use crate::cli;
use crate::error;
use crate::utils::{max_float, min_float};

pub use cli::PlotOptions;

fn default_accession() -> String {
    "draft".to_string()
}

fn default_level() -> String {
    "scaffold".to_string()
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AssemblyMeta {
    #[serde(default = "default_accession")]
    pub accession: String,
    #[serde(default = "default_level")]
    pub level: String,
    pub prefix: Option<String>,
    pub alias: Option<String>,
    pub bioproject: Option<String>,
    pub biosample: Option<String>,
    pub file: Option<PathBuf>,
    #[serde(rename = "scaffold-count")]
    pub scaffold_count: Option<usize>,
    pub span: Option<usize>,
    pub url: Option<Url>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Datatype {
    Float,
    Integer,
    Mixed,
    String,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FieldMeta {
    pub id: String,
    #[serde(rename = "type")]
    pub field_type: Option<String>,
    pub scale: Option<String>,
    pub datatype: Option<Datatype>,
    pub children: Option<Vec<FieldMeta>>,
    pub parent: Option<String>,
    pub data: Option<Vec<FieldMeta>>,
    pub count: Option<usize>,
    pub range: Option<[f64; 2]>,
    #[serde_as(deserialize_as = "DefaultOnError")]
    #[serde(default)]
    pub clamp: Option<f64>,
    pub preload: Option<bool>,
    pub active: Option<bool>,
    #[serde(rename = "set")]
    pub odb_set: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct PlotMeta {
    pub x: Option<String>,
    pub y: Option<String>,
    pub z: Option<String>,
    pub cat: Option<String>,
    pub labels: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TaxonMeta {
    #[serde(default = "default_taxname")]
    pub name: String,
    pub class: Option<String>,
    pub family: Option<String>,
    pub genus: Option<String>,
    pub kingdom: Option<String>,
    pub order: Option<String>,
    pub phylum: Option<String>,
    pub superkingdom: Option<String>,
    #[serde(
        default = "default_taxid",
        deserialize_with = "deserialize_string_from_number"
    )]
    pub taxid: String,
}

fn default_taxname() -> String {
    "unnamed".to_string()
}

fn default_taxid() -> String {
    "0".to_string()
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Meta {
    pub id: String,
    pub name: String,
    #[serde(default = "default_level")]
    pub record_type: String,
    pub records: usize,
    #[serde(default = "default_revision")]
    pub revision: u8,
    #[serde(default = "default_version")]
    pub version: u8,
    pub assembly: AssemblyMeta,
    pub fields: Vec<FieldMeta>,
    #[serde(default = "default_plotmeta")]
    pub plot: PlotMeta,
    pub taxon: TaxonMeta,
    pub field_list: Option<HashMap<String, FieldMeta>>,
    pub busco_list: Option<Vec<(String, usize, String)>>,
}

fn default_revision() -> u8 {
    0
}

fn default_version() -> u8 {
    1
}

fn default_plotmeta() -> PlotMeta {
    PlotMeta {
        ..Default::default()
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Field<T> {
    // pub meta: FieldMeta,
    pub values: Vec<T>,
    pub keys: Vec<String>,
    pub category_slot: Option<u8>,
    pub headers: Option<Vec<String>>,
}

impl<T> Field<T> {
    pub fn values(&self) -> &Vec<T> {
        &self.values
    }
}

#[derive(Debug)]
pub struct Filter {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub invert: bool,
    pub key: Option<Vec<String>>,
}

impl Default for Filter {
    fn default() -> Filter {
        Filter {
            min: None,
            max: None,
            invert: false,
            key: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuscoGene {
    pub id: String,
    pub status: String,
}

pub fn get_path(dir: &PathBuf, prefix: &str) -> Option<String> {
    let mut path = dir.clone();
    path.push(prefix);
    for e in glob(&format!("{}*", path.to_string_lossy())).expect("Failed to read glob pattern") {
        return Some(format!("{}", e.unwrap().to_string_lossy()));
    }
    None
}

pub fn file_reader(dir: &PathBuf, prefix: &str) -> Option<Box<dyn BufRead>> {
    let blobdir = dir.to_str().unwrap();
    if blobdir.starts_with("http") {
        let mut url = format!("{}", dir.to_str().unwrap());
        if !prefix.starts_with("meta.") {
            url = format!(
                "{}/{}",
                url.replace("/dataset/id/", "/field/"),
                prefix.replace(".json", "")
            );
        }
        let response = reqwest::blocking::get(&url).expect("Failed to fetch file");
        if response.status().is_success() {
            return Some(Box::new(BufReader::new(response)));
        } else {
            return None;
        }
    } else {
        let path = match get_path(dir, prefix) {
            Some(string) => string,
            None => return None,
        };
        let file = File::open(&path).expect("no such file");

        if path.ends_with(".gz") {
            return Some(Box::new(BufReader::new(GzDecoder::new(file))));
        } else {
            return Some(Box::new(BufReader::new(file)));
        }
    }
}

pub fn local_file_reader(dir: &PathBuf, prefix: &str) -> Option<Box<dyn BufRead>> {
    let path = match get_path(dir, prefix) {
        Some(string) => string,
        None => return None,
    };
    let file = File::open(&path).expect("no such file");

    if path.ends_with(".gz") {
        return Some(Box::new(BufReader::new(GzDecoder::new(file))));
    } else {
        return Some(Box::new(BufReader::new(file)));
    }
}

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct Keys {
    pub headers: String,
}

/// Parse a blobdir
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// # use crate::blobtk::blobdir::parse_blobdir;
/// let meta = parse_blobdir(&PathBuf::from("test/minimal")).unwrap();
/// assert_eq!(meta.taxon.name, "unnamed".to_string());
/// ```

pub fn parse_blobdir(blobdir: &PathBuf) -> Result<Meta, error::Error> {
    let reader = match file_reader(blobdir, "meta.json") {
        Some(r) => r,
        None => {
            return Err(error::Error::FileNotFound(format!(
                "{}/meta.json",
                &blobdir.to_str().unwrap()
            )))
        } // }
    };
    let mut meta: Meta = match serde_json::from_reader(reader) {
        Ok(meta) => meta,
        Err(err) => {
            return Err(error::Error::SerdeError(format!(
                "{}/meta.json {}",
                &blobdir.to_str().unwrap(),
                err.to_string()
            )))
        }
    };
    let mut fields: HashMap<String, FieldMeta> = HashMap::new();
    let mut busco_fields: Vec<(String, usize, String)> = vec![];
    fn list_fields(
        field_list: &Vec<FieldMeta>,
        fields: &mut HashMap<String, FieldMeta>,
        busco_fields: &mut Vec<(String, usize, String)>,
        busco: bool,
        parent: Option<&FieldMeta>,
    ) {
        for f in field_list {
            // let iterable_headers: HashMap<String, String> =
            //     serde_json::from_value(serde_json::to_value(&f).unwrap()).unwrap();
            let full = if parent.is_none() {
                f.clone()
            } else {
                let mut tmp = FieldMeta {
                    id: f.id.clone(),
                    children: f.children.clone(),
                    parent: f.parent.clone(),
                    data: f.data.clone(),
                    ..parent.unwrap().clone()
                };
                if f.field_type.is_some() {
                    tmp.field_type = f.field_type.clone()
                }
                if f.range.is_some() {
                    tmp.range = f.range.clone()
                }
                if f.preload.is_some() {
                    tmp.preload = f.preload.clone()
                }
                if f.active.is_some() {
                    tmp.active = f.active.clone()
                }
                if f.scale.is_some() {
                    tmp.scale = f.scale.clone()
                }
                if f.datatype.is_some() {
                    tmp.datatype = f.datatype.clone()
                }
                if f.count.is_some() {
                    tmp.count = f.count.clone()
                }
                if f.odb_set.is_some() {
                    tmp.odb_set = f.odb_set.clone()
                }
                tmp
            };

            // full = FieldMeta { id: f.id, ..parent };
            let busco_flag = if f.id == "busco".to_string() {
                true
            } else {
                busco
            };
            if f.children.is_none() {
                fields.insert(f.id.clone(), full.clone());
                if busco_flag {
                    busco_fields.push((
                        f.id.clone(),
                        f.count.unwrap_or(1),
                        f.odb_set.clone().unwrap(),
                    ));
                }
            } else {
                list_fields(
                    &f.children.clone().unwrap(),
                    fields,
                    busco_fields,
                    busco_flag,
                    Some(&full),
                )
            }
            if f.data.is_some() {
                list_fields(
                    &f.data.clone().unwrap(),
                    fields,
                    busco_fields,
                    busco_flag,
                    Some(&full),
                )
            }
        }
    }
    list_fields(&meta.fields, &mut fields, &mut busco_fields, false, None);
    meta.field_list = Some(fields);
    meta.busco_list = Some(busco_fields);
    if meta.record_type != "scaffold" {
        meta.record_type = if titlecase(&meta.assembly.level) == "Contig".to_string() {
            "contig".to_string()
        } else {
            "scaffold".to_string()
        };
    }

    Ok(meta)
}

pub fn parse_field_busco(id: String, blobdir: &PathBuf) -> Option<Vec<Vec<BuscoGene>>> {
    let reader = match file_reader(blobdir, &format!("{}.json", &id)) {
        Some(reader) => reader,
        None => return None,
    };
    let field: Field<Vec<(String, usize)>> =
        serde_json::from_reader(reader).expect("unable to parse json");
    let mut values: Vec<Vec<BuscoGene>> = vec![];
    let keys = field.keys.clone();
    // let cat_slot = field.category_slot.unwrap() as usize;
    for value in field.values() {
        let mut val = vec![];
        for v in value {
            val.push(BuscoGene {
                id: v.0.clone(),
                status: keys[v.1].clone(),
            });
        }
        values.push(val);
    }
    Some(values)
}

pub fn parse_field_cat(
    id: String,
    blobdir: &PathBuf,
) -> Result<Vec<(String, usize)>, error::Error> {
    let reader = match file_reader(blobdir, &format!("{}.json", &id)) {
        Some(reader) => reader,
        None => {
            return Err(error::Error::FileNotFound(format!(
                "{}/{}.json",
                &blobdir.to_str().unwrap(),
                &id
            )))
        }
    };
    let field: Field<usize> = serde_json::from_reader(reader).expect("unable to parse json");
    let mut values: Vec<(String, usize)> = vec![];
    let keys = field.keys.clone();
    for value in field.values() {
        values.push((keys[*value].clone(), *value))
    }
    Ok(values)
}

pub fn parse_field_cat_windows(
    id: String,
    blobdir: &PathBuf,
    wanted_indices: &Vec<usize>,
) -> Result<Vec<Vec<Option<(String, usize)>>>, error::Error> {
    let reader = match file_reader(blobdir, &format!("{}.json", &id)) {
        Some(reader) => reader,
        None => {
            return Err(error::Error::FileNotFound(format!(
                "{}/{}.json",
                &blobdir.to_str().unwrap(),
                &id
            )))
        }
    };
    let field: Field<Vec<Vec<Option<usize>>>> =
        serde_json::from_reader(reader).expect("unable to parse json");
    let mut values: Vec<Vec<Option<(String, usize)>>> = vec![];
    let keys = field.keys.clone();
    let cat_slot;
    if let Some(slot) = field.category_slot {
        cat_slot = slot as usize;
    } else {
        // TODO: raise error here
        return Ok(values);
    }
    let indices: HashSet<&usize> = HashSet::from_iter(wanted_indices);
    for (i, seq) in field.values().iter().enumerate() {
        if !indices.contains(&i) {
            continue;
        }
        let mut windows = vec![];
        for arr in seq {
            let value = match arr[cat_slot] {
                Some(v) => Some((keys[v].clone(), v)),
                None => None,
            };
            windows.push(value);
        }
        values.push(windows);
    }
    Ok(values)
}

pub fn parse_field_synonym(
    field_name: String,
    blobdir: &PathBuf,
) -> Result<Vec<Option<String>>, error::Error> {
    let mut id = field_name.clone();
    let mut name_header = None;
    if field_name.contains(".") {
        let (new_id, new_name_header) = field_name.split_once(".").unwrap();
        id = new_id.to_string();
        name_header = Some(new_name_header.to_string());
    }

    let reader = match file_reader(blobdir, &format!("{}.json", &id)) {
        Some(reader) => reader,
        None => {
            return Err(error::Error::FileNotFound(format!(
                "{}/{}.json",
                &blobdir.to_str().unwrap(),
                &id
            )))
        }
    };
    let field: Field<Vec<String>> = serde_json::from_reader(reader).expect("unable to parse json");
    let mut values: Vec<Option<String>> = vec![];
    let mut name_slot = 0;
    if let Some(headers) = field.headers.clone() {
        if let Some(n_header) = name_header {
            if let Some(slot) = headers.iter().position(|x| x == &n_header) {
                name_slot = slot;
            }
        }
    }
    for value in field.values() {
        values.push(if value.len() >= name_slot + 1 {
            Some(value[name_slot].clone())
        } else {
            None
        })
    }
    Ok(values)
}

pub fn parse_field_float(id: String, blobdir: &PathBuf) -> Result<Vec<f64>, error::Error> {
    let reader = match file_reader(blobdir, &format!("{}.json", &id)) {
        Some(reader) => reader,
        None => {
            return Err(error::Error::FileNotFound(format!(
                "{}/{}.json",
                &blobdir.to_str().unwrap(),
                &id
            )))
        }
    };
    let field: Field<f64> = serde_json::from_reader(reader).expect("unable to parse json");
    let values = field.values().clone();
    Ok(values)
}

pub fn parse_field_float_windows(
    id: String,
    blobdir: &PathBuf,
    wanted_indices: Option<&Vec<usize>>,
) -> Result<(Vec<Vec<f64>>, f64, f64), error::Error> {
    let reader = match file_reader(blobdir, &format!("{}.json", &id)) {
        Some(reader) => reader,
        None => {
            return Err(error::Error::FileNotFound(format!(
                "{}/{}.json",
                &blobdir.to_str().unwrap(),
                &id
            )))
        }
    };
    let field: Field<Vec<Vec<f64>>> =
        serde_json::from_reader(reader).expect("unable to parse json");
    let mut values = vec![];
    let indices: HashSet<usize> = match wanted_indices {
        Some(i) => HashSet::from_iter(i.iter().cloned()),
        None => HashSet::from_iter(0..field.values().len()),
    };
    let mut min_value = INFINITY;
    let mut max_value = NEG_INFINITY;
    for (i, seq) in field.values().iter().enumerate() {
        if !indices.contains(&i) {
            continue;
        }
        let mut windows = vec![];
        for arr in seq {
            windows.push(arr[0]);
            min_value = min_float(min_value, arr[0]);
            max_value = max_float(max_value, arr[0]);
        }
        values.push(windows);
    }
    Ok((values, min_value, max_value))
}

pub fn parse_field_int(id: String, blobdir: &PathBuf) -> Result<Vec<usize>, error::Error> {
    let reader = match file_reader(blobdir, &format!("{}.json", &id)) {
        Some(reader) => reader,
        None => {
            return Err(error::Error::FileNotFound(format!(
                "{}/{}.json",
                &blobdir.to_str().unwrap(),
                &id
            )))
        }
    };
    let field: Field<usize> = serde_json::from_reader(reader).expect("unable to parse json");
    let values = field.values().clone();
    Ok(values)
}

pub fn parse_field_int_windows(
    id: String,
    blobdir: &PathBuf,
    wanted_indices: Option<&Vec<usize>>,
) -> Result<(Vec<Vec<usize>>, usize, usize), error::Error> {
    let reader = match file_reader(blobdir, &format!("{}.json", &id)) {
        Some(reader) => reader,
        None => {
            return Err(error::Error::FileNotFound(format!(
                "{}/{}.json",
                &blobdir.to_str().unwrap(),
                &id
            )))
        }
    };
    let field: Field<Vec<Vec<usize>>> =
        serde_json::from_reader(reader).expect("unable to parse json");
    let mut values = vec![];
    let indices: HashSet<usize> = match wanted_indices {
        Some(i) => HashSet::from_iter(i.iter().cloned()),
        None => HashSet::from_iter(0..field.values().len()),
    };
    let mut min_value = INFINITY as usize;
    let mut max_value = NEG_INFINITY as usize;
    for (i, seq) in field.values().iter().enumerate() {
        if !indices.contains(&i) {
            continue;
        }
        let mut windows = vec![];
        for arr in seq {
            windows.push(arr[0]);
            min_value = min(min_value, arr[0]);
            max_value = max(max_value, arr[0]);
        }
        values.push(windows);
    }
    Ok((values, min_value, max_value))
}

pub fn parse_field_string(
    id: String,
    blobdir: &PathBuf,
) -> Result<(HashMap<String, usize>, Vec<String>), error::Error> {
    let reader = match file_reader(blobdir, &format!("{}.json", &id)) {
        Some(reader) => reader,
        None => {
            return Err(error::Error::FileNotFound(format!(
                "{}/{}.json",
                &blobdir.to_str().unwrap(),
                &id
            )))
        }
    };
    let field: Field<usize> = serde_json::from_reader(reader).expect("unable to parse json");
    let mut keys = HashMap::new();
    for (index, key) in field.keys.iter().enumerate() {
        keys.insert(key.clone(), index);
    }
    let values: Vec<String> = field
        .values()
        .iter()
        .map(|i| field.keys[i.to_owned()].clone())
        .collect();
    Ok((keys, values))
}

pub fn parse_field_identifiers(id: String, blobdir: &PathBuf) -> Result<Vec<String>, error::Error> {
    let reader = match file_reader(blobdir, &format!("{}.json", &id)) {
        Some(reader) => reader,
        None => {
            return Err(error::Error::FileNotFound(format!(
                "{}/{}.json",
                &blobdir.to_str().unwrap(),
                &id
            )))
        }
    };
    let field: Field<String> = serde_json::from_reader(reader).expect("unable to parse json");
    Ok(field.values().to_owned())
}

pub fn parse_filters(
    options: &cli::PlotOptions,
    plot_meta: Option<&HashMap<String, String>>,
) -> HashMap<String, Filter> {
    let mut filters = options.filter.clone();
    if plot_meta.is_some() && options.x_limit.is_some() {
        if let Some((min_value, max_value)) = options.x_limit.clone().unwrap().split_once(",") {
            let x_field = plot_meta.unwrap().get("x").unwrap();
            if !min_value.is_empty() {
                filters.push(format!("{}--Min={}", x_field, min_value))
            }
            if !max_value.is_empty() {
                filters.push(format!("{}--Max={}", x_field, max_value))
            }
        }
    }
    if plot_meta.is_some() && options.y_limit.is_some() {
        if let Some((min_value, max_value)) = options.y_limit.clone().unwrap().split_once(",") {
            let y_field = plot_meta.unwrap().get("y").unwrap();
            if !min_value.is_empty() {
                filters.push(format!("{}--Min={}", y_field, min_value))
            }
            if !max_value.is_empty() {
                filters.push(format!("{}--Max={}", y_field, max_value))
            }
        }
    }
    let mut filter_map = HashMap::new();
    for filter in filters.iter() {
        if let Some((id, parameter)) = filter.split_once("--") {
            if !filter_map.contains_key(id) {
                filter_map.insert(
                    id.to_string(),
                    Filter {
                        ..Default::default()
                    },
                );
            };
            let filter_params = filter_map.get_mut(&id.to_string()).unwrap();
            if parameter == "Inv" {
                filter_params.invert = true;
                continue;
            };
            if let Some((param, value)) = parameter.split_once("=") {
                match param {
                    "Max" => filter_params.max = Some(value.parse().unwrap()),
                    "Min" => filter_params.min = Some(value.parse().unwrap()),
                    "Keys" => {
                        filter_params.key = Some(
                            value
                                .split(",")
                                .map(|x| x.parse::<String>().unwrap())
                                .collect(),
                        )
                    }
                    "Inv" => {
                        filter_params.key = Some(
                            value
                                .split(",")
                                .map(|x| x.parse::<String>().unwrap())
                                .collect(),
                        );
                        filter_params.invert = true
                    }
                    _ => (),
                }
            }
        };
    }

    filter_map
}

// TODO: add filters for int and cat values
pub fn filter_float_values(values: Vec<f64>, filter: Filter, indices: Vec<usize>) -> Vec<usize> {
    let initial: Vec<usize> = if indices.is_empty() {
        (0..(values.len() - 1)).collect()
    } else {
        indices.clone()
    };
    let mut output = vec![];
    for i in initial {
        let mut keep = true;
        if filter.max.is_some() {
            if values[i] > filter.max.unwrap() {
                keep = false;
            }
        }
        if filter.min.is_some() {
            if values[i] < filter.min.unwrap() {
                keep = false;
            }
        }
        if filter.invert {
            keep = !keep;
        }
        if keep {
            output.push(i);
        }
    }
    output
}

pub fn filter_int_values(values: Vec<usize>, filter: Filter, indices: Vec<usize>) -> Vec<usize> {
    let initial: Vec<usize> = if indices.is_empty() {
        (0..(values.len() - 1)).collect()
    } else {
        indices.clone()
    };
    let mut output = vec![];
    for i in initial {
        let mut keep = true;
        if filter.max.is_some() {
            if values[i] as f64 > filter.max.unwrap() {
                keep = false;
            }
        }
        if filter.min.is_some() {
            if (values[i] as f64) < filter.min.unwrap() {
                keep = false;
            }
        }
        if filter.invert {
            keep = !keep;
        }
        if keep {
            output.push(i);
        }
    }
    output
}

pub fn filter_string_values(
    values: Vec<String>,
    keys: HashMap<String, usize>,
    filter: Filter,
    indices: Vec<usize>,
) -> Vec<usize> {
    let initial: Vec<usize> = if indices.is_empty() {
        (0..(values.len() - 1)).collect()
    } else {
        indices.clone()
    };
    let mut output = vec![];
    let ints: Vec<usize> = values
        .iter()
        .map(|x| match x.parse::<usize>() {
            Ok(value) => value,
            Err(_) => keys[x],
        })
        .collect();
    let set: HashSet<usize> = filter
        .key
        .clone()
        .unwrap()
        .iter()
        .map(|x| match x.parse::<usize>() {
            Ok(value) => value,
            Err(_) => keys[x],
        })
        .collect();
    for i in initial {
        let mut keep = true;
        if filter.key.is_some() {
            if set.contains(&ints[i]) {
                keep = false;
            }
        }
        // if filter.min.is_some() {
        //     if (values[i] as f64) < filter.min.unwrap() {
        //         keep = false;
        //     }
        // }
        if filter.invert {
            keep = !keep;
        }
        if keep {
            output.push(i);
        }
    }
    output
}

pub fn set_filters(filters: HashMap<String, Filter>, meta: &Meta, blobdir: &PathBuf) -> Vec<usize> {
    let mut indices = vec![];
    let field_list = meta.field_list.clone().unwrap();
    for (id, filter) in filters {
        let field_meta_option = field_list.get(&id);
        match field_meta_option {
            Some(field_meta) => {
                let field = field_meta.clone();
                match field.datatype {
                    Some(Datatype::Float) => {
                        let values = parse_field_float(field_meta.id.clone(), blobdir).unwrap();
                        indices = filter_float_values(values, filter, indices);
                    }
                    Some(Datatype::Integer) => {
                        let values = parse_field_int(field_meta.id.clone(), blobdir).unwrap();
                        indices = filter_int_values(values, filter, indices);
                    }
                    Some(Datatype::String) => {
                        let (keys, values) =
                            parse_field_string(field_meta.id.clone(), blobdir).unwrap();
                        indices = filter_string_values(values, keys, filter, indices);
                    }
                    Some(_) => (),
                    None => (),
                }
            }
            None => (),
        };
    }
    if indices.is_empty() {
        indices = (0..meta.records).collect();
    }
    indices
}

pub fn apply_filter_float(values: &Vec<f64>, indices: &Vec<usize>) -> Vec<f64> {
    let mut output = vec![];
    for i in indices {
        output.push(values[i.clone()])
    }
    output
}

pub fn apply_filter_int(values: &Vec<usize>, indices: &Vec<usize>) -> Vec<usize> {
    let mut output = vec![];
    for i in indices {
        output.push(values[i.clone()])
    }
    output
}

pub fn apply_filter_option_int(values: &Vec<Option<usize>>, indices: &Vec<usize>) -> Vec<usize> {
    let mut output = vec![];
    for i in indices {
        output.push(match values[i.clone()] {
            Some(v) => v,
            _ => 0,
        })
    }
    output
}

pub fn apply_filter_busco(
    values: &Vec<Vec<BuscoGene>>,
    indices: &Vec<usize>,
) -> Vec<Vec<BuscoGene>> {
    let mut output = vec![];
    for i in indices {
        output.push(values[i.clone()].clone())
    }
    output
}

pub fn apply_filter_cat(values: &Vec<(String, usize)>, indices: &Vec<usize>) -> Vec<String> {
    let mut output = vec![];
    for i in indices {
        output.push(values[i.clone()].clone().0)
    }
    output
}

pub fn apply_filter_cat_tuple(
    values: &Vec<(String, usize)>,
    indices: &Vec<usize>,
) -> Vec<(String, usize)> {
    let mut output = vec![];
    for i in indices {
        output.push(values[i.clone()].clone())
    }
    output
}

pub fn apply_filter_string(values: &Vec<String>, indices: &Vec<usize>) -> Vec<String> {
    let mut output = vec![];
    for i in indices {
        output.push(values[i.clone()].clone())
    }
    output
}

pub fn apply_filter_option_string_with_fallback(
    values: &Vec<Option<String>>,
    indices: &Vec<usize>,
    fallback: &Vec<String>,
) -> Vec<String> {
    let mut output = vec![];
    for i in indices {
        output.push(match &values[i.clone()] {
            Some(v) => v.clone(),
            _ => fallback[i.clone()].clone(),
        })
    }
    output
}

pub fn get_plot_values(
    meta: &Meta,
    blobdir: &PathBuf,
    plot_map: &HashMap<String, String>,
) -> Result<(HashMap<String, Vec<f64>>, Vec<(String, usize)>), error::Error> {
    let mut plot_values = HashMap::new();
    let mut cat_values = vec![];
    let field_list = meta.field_list.clone().unwrap();
    for (axis, id) in plot_map {
        let field_meta_option = field_list.get(id);
        match field_meta_option {
            Some(field_meta) => {
                let field = field_meta.clone();
                match field.datatype {
                    Some(Datatype::Float) => {
                        let values = parse_field_float(field_meta.id.clone(), blobdir)?;
                        plot_values.insert(axis.clone(), values);
                    }
                    Some(Datatype::Integer) => {
                        let values: Vec<f64> = parse_field_int(field_meta.id.clone(), blobdir)?
                            .iter()
                            .map(|x| x.clone() as f64)
                            .collect();
                        plot_values.insert(axis.clone(), values);
                    }
                    Some(Datatype::String) => {
                        if field.data.is_some() {
                            cat_values = parse_field_cat(field_meta.id.clone(), blobdir)?;
                        }
                    }
                    Some(_) => (),
                    None => (),
                }
            }
            None => {
                if axis == "cat" && id == "_" {
                    cat_values = vec![("blank".to_string(), 0); meta.records]
                } else {
                    ()
                }
            }
        };
    }
    Ok((plot_values, cat_values))
}

pub fn get_window_values(
    meta: &Meta,
    blobdir: &PathBuf,
    plot_map: &HashMap<String, String>,
    wanted_indices: &Vec<usize>,
    window_size: &Option<String>,
) -> Result<
    (
        HashMap<String, Vec<Vec<f64>>>,
        Vec<Vec<Option<(String, usize)>>>,
        HashMap<String, [f64; 2]>,
    ),
    error::Error,
> {
    let mut plot_values = HashMap::new();
    let mut axis_limits = HashMap::new();
    let mut cat_values = vec![];
    let field_list = meta.field_list.clone().unwrap();
    let axes = vec!["x", "y", "z", "cat"];
    for axis in axes {
        let default_id = "_".to_string();
        let id = plot_map.get(axis).unwrap_or(&default_id);
        if id == "_" {
            continue;
        }
        let window_id = match window_size {
            Some(ref size) if size != "0.1" => format!("{}_windows_{}", id, size),
            _ => format!("{}_windows", id),
        };

        let field_meta_option = field_list.get(&window_id);
        match field_meta_option {
            Some(field_meta) => {
                let field = field_meta.clone();

                match field.datatype {
                    Some(Datatype::Mixed) => {
                        let (values, min_value, max_value) = parse_field_float_windows(
                            field_meta.id.clone(),
                            blobdir,
                            Some(wanted_indices),
                        )?;
                        plot_values.insert(axis.to_string(), values);
                        axis_limits.insert(axis.to_string(), [min_value, max_value]);
                    }
                    Some(Datatype::String) => {
                        cat_values = parse_field_cat_windows(
                            field_meta.id.clone(),
                            blobdir,
                            wanted_indices,
                        )?;
                        // cat_values.insert(axis.clone(), values);
                    }
                    // Some(Datatype::Integer) => {
                    //     let values: Vec<f64> =
                    //         parse_field_int_windows(field_meta.id.clone(), blobdir)?
                    //             .iter()
                    //             .map(|x| x.clone() as f64)
                    //             .collect();
                    //     plot_values.insert(axis.clone(), values);
                    // }
                    // Some(Datatype::String) => {
                    //     if field.data.is_some() {
                    //         cat_values = parse_field_cat_windows(field_meta.id.clone(), blobdir)?;
                    //     }
                    // }
                    Some(_) => (),
                    None => (),
                }
            }
            None => {
                if axis == "cat" && id == "_" {
                    cat_values = vec![vec![Some(("blank".to_string(), 0)); meta.records]]
                } else {
                    ()
                }
            }
        };
        if axis == "cat" && (cat_values.is_empty() || cat_values[0].is_empty()) {
            let window_counts = plot_values
                .get("x")
                .unwrap()
                .iter()
                .map(|x| x.len())
                .collect::<Vec<usize>>();
            let _single_cat_values = parse_field_cat(id.clone(), blobdir)?;
            let mut single_cat_values: Vec<(String, usize)> = vec![];
            for i in wanted_indices {
                single_cat_values.push(_single_cat_values[*i].clone());
            }
            for (i, window_count) in window_counts.iter().enumerate() {
                if *window_count > 0 {
                    cat_values.push(vec![Some(single_cat_values[i].clone()); *window_count]);
                } else {
                    cat_values.push(vec![None; *window_count]);
                }
            }
        }
    }
    Ok((plot_values, cat_values, axis_limits))
}
